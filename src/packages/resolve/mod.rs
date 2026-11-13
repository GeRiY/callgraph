//! Chain of Responsibility: melyik csomagé egy hívás célja? A láncszemek sorban kérdezik a hívás nyers minősítőjét
//! (PHP: a feloldott teljes osztálynév; JS / TS / Vue: a nem helyi import forrása); az első, amelyik felismeri,
//! dönt (`Some`), különben a következő jön (`None`). Új eset = új láncszem, a többi nem változik.
//!
//! A feloldás lekérdezéskor fut (a kimenet előtt), nem az elemzéskor: a projekt-gráf a nyers minősítőt tárolja, így
//! a `composer.lock` / `package-lock.json` változása nem hagy elavult csomagnevet a fájlonkénti rekordokban.
//!
//! - `facade.rs`: Laravel-homlokzat `use` nélkül (`Http::get`) → `laravel/framework`;
//! - `own.rs`: a projekt saját psr-4 / psr-0 névtere → saját kód;
//! - `psr4.rs`: a lock csomagjainak leghosszabb illeszkedő névtér-előtagja (a tranzitív csomagoké is);
//! - `bare_import.rs`: a JS csomag-import (`axios`, `@vueuse/core`, `lodash/debounce`) → a csomag neve.
//!
//! Minden hibatűrő: amit egyik láncszem sem ismer fel, az `Unknown` (nincs csomag), soha nem hiba.

mod bare_import;
mod facade;
mod own;
mod psr4;

#[cfg(test)]
mod tests;

use std::path::Path;

use super::model::{Ecosystem, Package};
use super::project::{EcosystemPackages, ProjectPackages};
use crate::model::Call;

use bare_import::BareImport;
use facade::FacadeTable;
use own::OwnNamespace;
use psr4::Psr4Prefix;

// a `deps` vizsgálat ugyanezzel a két szabállyal dönt a CSS-importról és a beépített modulról
pub(crate) use bare_import::{is_builtin, package_name};

/// A lánc, sorrendben. A homlokzat a saját névtér előtt áll: a `use` nélkül, névtérben írt `Http::get`-et az
/// elemző a fájl névterébe oldja (`App\Services\Http`), amit a saját névtér különben elvinne.
const CHAIN: &[&dyn SymbolResolver] = &[&FacadeTable, &OwnNamespace, &Psr4Prefix, &BareImport];

/// A rövid címke helyett a gyártó / scope áll, ha a név utolsó tagja ilyen általános szó (`laravel/framework` →
/// `laravel`, `@vueuse/core` → `vueuse`).
const GENERIC_SEGMENTS: &[&str] = &[
    "framework",
    "core",
    "support",
    "sdk",
    "client",
    "lib",
    "api",
];

/// Egy hívás célja a csomag-feloldáshoz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SymbolRef<'a> {
    /// A hívó fájl útja: a hozzá legközelebbi csomag-rész dönt (monorepo).
    pub path: &'a Path,
    pub ecosystem: Ecosystem,
    /// PHP: a teljes osztálynév (vagy névteres függvénynév) vezető `\` nélkül; Node: az import forrás-specifikátora.
    pub qualifier: &'a str,
    /// PHP: a forrásban minősítetlenül, `use` nélkül írt rövid osztálynév (homlokzat-jelölt: `Http`).
    pub alias: Option<&'a str>,
}

/// A feloldás eredménye.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Resolution {
    /// Egy csomagé (a nem deklarált JS-import is: a neve a specifikátorból).
    Package(Attribution),
    /// A projekt saját kódja.
    Own,
    /// Nem eldönthető (vagy nem csomag: Node beépített modul, globális függvény).
    Unknown,
}

/// A hívás csomagja: a név és — ha a legközelebbi rész lockja ismeri — a telepített verzió.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attribution {
    pub name: String,
    pub version: Option<String>,
}

/// Egy láncszem: felismeri-e a szimbólumot a fájlhoz legközelebbi csomag-rész alapján; `None` = a következő jön.
pub(crate) trait SymbolResolver {
    fn resolve(&self, sym: &SymbolRef<'_>, part: &EcosystemPackages) -> Option<Resolution>;
}

/// A szimbólum feloldása a lánccal; az ökoszisztémájában a fájlhoz nincs csomag-rész → `Unknown`.
pub(crate) fn resolve(sym: &SymbolRef<'_>, map: &ProjectPackages) -> Resolution {
    let Some(part) = map.nearest(sym.path, sym.ecosystem) else {
        return Resolution::Unknown;
    };
    CHAIN
        .iter()
        .find_map(|link| link.resolve(sym, part))
        .unwrap_or(Resolution::Unknown)
}

impl<'a> SymbolRef<'a> {
    /// A tárolt hívás nyers minősítője: JS-nél a nem helyi import forrása (`Call::from`), PHP-fájlban a feloldott
    /// osztály (`resolved` `::` előtti része; `new`-nál maga a `resolved`), a feloldatlan függvényhívásnál a név.
    /// A dinamikus hívás és a feloldatlan fogadójú metódushívás: `None`.
    pub(crate) fn of_call(path: &'a Path, call: &'a Call) -> Option<Self> {
        if call.is_dynamic() {
            return None;
        }
        if let Some(spec) = call.from.as_deref() {
            // egy ref / computed `.value`-ján át hívott metódus a saját adaté (`computed(…).value.pages.filter`),
            // nem a csomag API-ja
            if call
                .resolved
                .as_deref()
                .is_some_and(|r| r.contains("().value."))
            {
                return None;
            }
            return Some(Self {
                path,
                ecosystem: Ecosystem::Node,
                qualifier: spec,
                alias: None,
            });
        }
        if !is_php(path) {
            return None;
        }
        let qualifier = match (call.kind.as_str(), call.resolved.as_deref()) {
            (_, Some(resolved)) => resolved.split("::").next().unwrap_or(resolved),
            ("func_call", None) => call.target.as_str(),
            _ => return None,
        };
        let qualifier = qualifier.trim_start_matches('\\');
        if qualifier.is_empty() {
            return None;
        }
        // a cél az osztálynévvel kezdődik (`Http::get`, `new Client`); a metódushívás célja csak a metódus neve
        let class_target = matches!(
            call.kind.as_str(),
            "static_call" | "new" | "const_fetch" | "static_property_fetch"
        );
        let alias = if class_target {
            unimported_short_name(call, qualifier)
        } else {
            None
        };
        Some(Self {
            path,
            ecosystem: Ecosystem::Php,
            qualifier,
            alias,
        })
    }

    /// Egy import (a fájl `imports` térképének értéke) nyers minősítője: PHP-fájlban a `use` teljes neve, JS / TS /
    /// Vue-fájlban az érték forrás-része (`@vueuse/core::useMouse` → `@vueuse/core`). Más nyelv: `None`.
    pub(crate) fn of_import(path: &'a Path, full: &'a str) -> Option<Self> {
        let (ecosystem, qualifier) = if is_php(path) {
            (Ecosystem::Php, full.trim_start_matches('\\'))
        } else if is_script(path) {
            (Ecosystem::Node, full.split("::").next().unwrap_or(full))
        } else {
            return None;
        };
        (!qualifier.is_empty()).then_some(Self {
            path,
            ecosystem,
            qualifier,
            alias: None,
        })
    }
}

/// A cél rövid osztályneve, ha `use` nélkül, minősítetlenül írták: a globális névtérben a feloldott név maga a rövid
/// név; névtérben a kategorizáló jelölte (`framework_use`: ismert homlokzat, import nélkül — vagy importált
/// framework-osztály, amelyet a homlokzat-tábla úgyis ugyanarra a csomagra visz).
fn unimported_short_name<'a>(call: &'a Call, qualifier: &str) -> Option<&'a str> {
    let short = call.target.split("::").next()?;
    let plain = !short.is_empty()
        && !short.contains('\\')
        && !["self", "static", "parent"]
            .iter()
            .any(|word| short.eq_ignore_ascii_case(word));
    (plain && (qualifier == short || call.category == "framework_use")).then_some(short)
}

/// PHP-fájl-e az út (a `.blade.php` is).
fn is_php(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("php"))
}

/// JS / TS / Vue / Svelte-fájl-e az út (az import-térképe npm-csomagokra mutathat).
fn is_script(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            [
                "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "vue", "svelte",
            ]
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// A PHP-név a névtér-előtag alatt van-e (a PHP-névterek kis- / nagybetű-függetlenek); az üres előtag semmit.
fn under(qualifier: &str, prefix: &str) -> bool {
    !prefix.is_empty()
        && qualifier
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

impl Attribution {
    /// Egy csomag-bejegyzésből (a lock pontos verziójával, ha van).
    pub(crate) fn of(package: &Package) -> Self {
        Self {
            name: package.name.clone(),
            version: package.installed.clone(),
        }
    }

    /// A rövid címke a kimenetben: a név utolsó tagja (`guzzlehttp/guzzle` → `guzzle`), általános utolsó tagnál a
    /// gyártó / scope (`laravel/framework` → `laravel`).
    pub(crate) fn label(&self) -> &str {
        let (vendor, last) = self.parts();
        match vendor {
            Some(vendor) if GENERIC_SEGMENTS.contains(&last.to_ascii_lowercase().as_str()) => {
                vendor
            }
            _ => last,
        }
    }

    /// Illik-e rá egy (kisbetűs) keresőkifejezés: a teljes név, a címke, a gyártó / scope, az utolsó tag vagy annak
    /// első szava (`stripe/stripe-php` → `stripe`, `inertiajs/inertia-laravel` → `inertia`).
    pub(crate) fn matches_term(&self, term: &str) -> bool {
        let (vendor, last) = self.parts();
        let first_word = last.split(['-', '_', '.']).next().unwrap_or(last);
        [
            Some(self.name.as_str()),
            Some(self.label()),
            vendor,
            Some(last),
            Some(first_word),
        ]
        .into_iter()
        .flatten()
        .any(|name| !name.is_empty() && name.eq_ignore_ascii_case(term))
    }

    /// (gyártó / scope `@` nélkül, utolsó tag).
    fn parts(&self) -> (Option<&str>, &str) {
        match self.name.rsplit_once('/') {
            Some((vendor, last)) => (Some(vendor.trim_start_matches('@')), last),
            None => (None, self.name.as_str()),
        }
    }
}
