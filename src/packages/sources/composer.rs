//! Composer: a `composer.json` (`require` / `require-dev` a platform-bejegyzések — `php`, `ext-*`, `lib-*` — nélkül;
//! a saját `autoload` psr-4 / psr-0 névterek) és a `composer.lock` (`packages` + `packages-dev`: pontos verzió vezető
//! `v` nélkül, a csomag psr-4 / psr-0 névterei, az `extra.laravel` auto-discovery és a `composer-plugin` típus).

use serde_json::Value;

use super::{LockSource, ManifestSource, push_deps, text_field};
use crate::packages::model::{Installed, Manifest};

/// A `composer.json` stratégiája.
pub(crate) struct Composer;

/// A `composer.lock` stratégiája.
pub(crate) struct ComposerLock;

impl ManifestSource for Composer {
    fn file(&self) -> &'static str {
        "composer.json"
    }

    fn read(&self, text: &str) -> Option<Manifest> {
        let value: Value = serde_json::from_str(text).ok()?;
        if !value.is_object() {
            return None;
        }
        let mut deps = Vec::new();
        push_deps(&mut deps, value.get("require"), false, is_package);
        push_deps(&mut deps, value.get("require-dev"), true, is_package);
        let mut own_namespaces = autoload_namespaces(value.get("autoload"));
        for prefix in autoload_namespaces(value.get("autoload-dev")) {
            if !own_namespaces.contains(&prefix) {
                own_namespaces.push(prefix);
            }
        }
        Some(Manifest {
            deps,
            own_namespaces,
            name: text_field(&value, "name"),
            workspaces: Vec::new(),
        })
    }
}

impl LockSource for ComposerLock {
    fn file(&self) -> &'static str {
        "composer.lock"
    }

    fn read(&self, text: &str) -> Vec<Installed> {
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            return Vec::new();
        };
        ["packages", "packages-dev"]
            .iter()
            .filter_map(|section| value.get(*section)?.as_array())
            .flatten()
            .filter_map(|entry| {
                let name = entry.get("name")?.as_str()?;
                let version = entry
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Some(Installed {
                    name: name.to_owned(),
                    version: strip_v(version).to_owned(),
                    namespaces: autoload_namespaces(entry.get("autoload")),
                    ranges: Vec::new(),
                    auto_discovered: auto_discovered(entry),
                    plugin: entry.get("type").and_then(Value::as_str) == Some("composer-plugin"),
                })
            })
            .collect()
    }
}

/// A bejegyzés `extra.laravel.providers` (vagy `aliases`) listája nem üres: a Laravel package auto-discovery tölti be.
fn auto_discovered(entry: &Value) -> bool {
    let Some(laravel) = entry.get("extra").and_then(|extra| extra.get("laravel")) else {
        return false;
    };
    ["providers", "aliases"]
        .iter()
        .any(|key| match laravel.get(*key) {
            Some(Value::Array(list)) => !list.is_empty(),
            Some(Value::Object(map)) => !map.is_empty(),
            _ => false,
        })
}

/// Valódi csomag-e (`vendor/név`); a platform-bejegyzéseknek (`php`, `ext-json`, `lib-icu`,
/// `composer-plugin-api`) nincs vendor-előtagjuk.
fn is_package(name: &str) -> bool {
    name.contains('/')
}

/// `v11.31.0` → `11.31.0`; a `dev-main` és a többi alak változatlan.
fn strip_v(version: &str) -> &str {
    match version.strip_prefix(['v', 'V']) {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => rest,
        _ => version,
    }
}

/// Az `autoload` psr-4 és psr-0 kulcsai, `\`-re végződve (a psr-0 `Twig_` alakú előtagja marad); az üres
/// (tartalék) előtag kimarad.
fn autoload_namespaces(autoload: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for kind in ["psr-4", "psr-0"] {
        let Some(map) = autoload
            .and_then(|a| a.get(kind))
            .and_then(Value::as_object)
        else {
            continue;
        };
        for prefix in map
            .keys()
            .filter_map(|key| normalize_namespace(key.as_str()))
        {
            if !out.contains(&prefix) {
                out.push(prefix);
            }
        }
    }
    out
}

fn normalize_namespace(prefix: &str) -> Option<String> {
    let prefix = prefix.trim().trim_start_matches('\\');
    if prefix.is_empty() {
        None
    } else if prefix.ends_with('\\') || prefix.ends_with('_') {
        Some(prefix.to_owned())
    } else {
        Some(format!("{prefix}\\"))
    }
}
