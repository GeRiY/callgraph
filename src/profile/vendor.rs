//! A profilból kimaradó kód: a beágyazott külső csomagok (vendor) és a generált mappák. A fájlok a térképben
//! maradnak (a lekérdezések látják őket), csak a profil rétegei, mintái és hívásiránya nem számol velük — egy
//! bemásolt SDK vagy framework-másolat ne adja a projekt „architektúráját”.
//!
//! - vendor: egy jól ismert nevű mappa (`vendors`, `third_party`, `extensions`, `external`, `libs`…; az egység az
//!   alatta lévő első mappa: `extensions/Facebook`), vagy egy almappa (nem a gyökér) saját könyvtár-`composer.json`-nal
//!   (nem `"type": "project"`, nincs a gyökér `replace` listájában, és más a vendor-előtagja, mint a gyökéré), saját,
//!   nem privát, verziózott `package.json`-nal (más a scope-ja, mint a gyökéré), vagy — manifest nélkül — saját
//!   licencfájllal (`LICENSE`, `LICENCE`, `COPYING`). A gyökér `package.json` `workspaces` és a `composer.json`
//!   `path`-repository mappái a projekt saját kódja.
//! - generált: `dist`, `build`, `*.min` nevű mappa (`assets.min`); a gráfban `binary` jelölésű (minifikált) fájl.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Mappanevek (kisbetűvel), amelyek alatt külső kód van; az egység a név alatti első mappa.
const VENDOR_NAMES: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "thirdparty",
    "extensions",
    "external",
    "libs",
];

/// Generált (build-kimenet) mappanevek, kisbetűvel; mellettük a `*.min` végű mappák.
const GENERATED_NAMES: &[&str] = &["dist", "build"];

/// Miért marad ki egy fájl a profilból, és melyik mappa az egysége (a kiírt listához).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Skip {
    Vendor(String),
    Generated(String),
}

/// A gyökér alatti vendor- / generált mappák felismerője; a mappánkénti döntés gyorsítótárban.
#[derive(Debug)]
pub(crate) struct Skipper {
    root: PathBuf,
    vendor: bool,
    own: Vec<String>,
    /// A gyökér `composer.json` `replace` csomagnevei (kisbetűvel): a projekt saját, felosztott csomagjai.
    replaced: Vec<String>,
    root_vendor: Option<String>,
    root_scope: Option<String>,
    cache: HashMap<String, bool>,
}

impl Skipper {
    /// `vendor: false` (`profile --with-vendor`): csak a generált mappák maradnak ki.
    pub(crate) fn new(root: &Path, vendor: bool) -> Self {
        let composer = read_json(&root.join("composer.json"));
        let package = read_json(&root.join("package.json"));
        let root_vendor = composer
            .as_ref()
            .and_then(|c| c.get("name")?.as_str().map(vendor_prefix));
        let root_scope = package
            .as_ref()
            .and_then(|p| p.get("name")?.as_str().and_then(npm_scope));
        let mut own = Vec::new();
        if let Some(package) = &package {
            let list = package
                .get("workspaces")
                .map(|w| w.get("packages").unwrap_or(w));
            own.extend(strings(list).map(|w| pattern_prefix(&w)));
        }
        if let Some(repos) = composer.as_ref().and_then(|c| c.get("repositories")) {
            let repos: Vec<&serde_json::Value> = match repos {
                serde_json::Value::Array(list) => list.iter().collect(),
                serde_json::Value::Object(map) => map.values().collect(),
                _ => Vec::new(),
            };
            for repo in repos {
                if repo.get("type").and_then(|t| t.as_str()) == Some("path")
                    && let Some(url) = repo.get("url").and_then(|u| u.as_str())
                {
                    own.push(pattern_prefix(url));
                }
            }
        }
        own.retain(|prefix| !prefix.is_empty());
        let replaced: Vec<String> = composer
            .as_ref()
            .and_then(|c| c.get("replace")?.as_object())
            .map(|map| map.keys().map(|k| k.to_ascii_lowercase()).collect())
            .unwrap_or_default();
        Self {
            root: root.to_path_buf(),
            vendor,
            own,
            replaced,
            root_vendor,
            root_scope,
            cache: HashMap::new(),
        }
    }

    /// Kimarad-e a (gyökérhez relatív, `/`-es) út a profilból; a legkülső illeszkedő mappa dönt.
    pub(crate) fn skip(&mut self, rel: &str) -> Option<Skip> {
        let (dir, _) = rel.rsplit_once('/')?;
        let segments: Vec<&str> = dir.split('/').collect();
        for (at, segment) in segments.iter().enumerate() {
            let name = segment.to_ascii_lowercase();
            let prefix = segments[..=at].join("/");
            if GENERATED_NAMES.contains(&name.as_str()) || name.ends_with(".min") {
                return Some(Skip::Generated(prefix));
            }
            if !self.vendor || self.is_own(&prefix) {
                continue;
            }
            if VENDOR_NAMES.contains(&name.as_str()) {
                let unit = segments[..=(at + 1).min(segments.len() - 1)].join("/");
                return Some(Skip::Vendor(unit));
            }
            if self.is_package(&prefix) {
                return Some(Skip::Vendor(prefix));
            }
        }
        None
    }

    /// A gyökér `workspaces` / `path`-repository mappái (és alattuk minden) a projekt saját kódja.
    fn is_own(&self, prefix: &str) -> bool {
        self.own
            .iter()
            .any(|own| prefix == own.as_str() || prefix.starts_with(&format!("{own}/")))
    }

    /// Saját licenc / könyvtár-manifest van-e a mappában (gyorsítótárazva).
    fn is_package(&mut self, prefix: &str) -> bool {
        if let Some(known) = self.cache.get(prefix) {
            return *known;
        }
        let dir = self.root.join(prefix);
        // a manifest dönt (egy saját, felosztott csomag mellett is lehet licenc); manifest nélkül a licenc
        let found = if let Some(json) = read_json(&dir.join("composer.json")) {
            self.foreign_composer(&json)
        } else if let Some(json) = read_json(&dir.join("package.json")) {
            self.foreign_package(&json)
        } else {
            has_license(&dir)
        };
        self.cache.insert(prefix.to_owned(), found);
        found
    }

    /// Könyvtár-`composer.json`, amely nem a projekté: nem `project` típusú, nem szerepel a gyökér `replace`
    /// listájában (felosztott monorepo, pl. a `laravel/framework` `illuminate/*` csomagjai), és más a vendor-előtagja.
    fn foreign_composer(&self, json: &serde_json::Value) -> bool {
        let project = json.get("type").and_then(|t| t.as_str()) == Some("project");
        let Some(name) = json.get("name").and_then(|n| n.as_str()) else {
            return false;
        };
        let name = name.to_ascii_lowercase();
        !project
            && !self.replaced.contains(&name)
            && self.root_vendor.as_deref() != Some(vendor_prefix(&name).as_str())
    }

    fn foreign_package(&self, json: &serde_json::Value) -> bool {
        let private = json.get("private").and_then(|p| p.as_bool()) == Some(true);
        let versioned = json.get("version").is_some();
        let Some(name) = json.get("name").and_then(|n| n.as_str()) else {
            return false;
        };
        let same_scope = self.root_scope.is_some() && npm_scope(name) == self.root_scope;
        !private && versioned && !same_scope
    }
}

/// Licencfájl a mappában: `LICENSE`, `LICENCE`, `COPYING` (kis-/nagybetűtől függetlenül), kiterjesztés nélkül vagy
/// szövegként (`.md`, `.txt`), illetve `LICENSE-MIT` alakban — egy `License.php` (domain-osztály) nem az.
fn has_license(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(|e| e.ok()).any(|entry| {
        is_license_name(&entry.file_name().to_string_lossy())
            && entry.file_type().is_ok_and(|t| t.is_file())
    })
}

fn is_license_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let (stem, ext) = name.split_once('.').unwrap_or((name.as_str(), ""));
    let stem = stem.split_once('-').map_or(stem, |(head, _)| head);
    ["license", "licence", "copying", "unlicense"].contains(&stem)
        && ["", "md", "txt", "rst", "markdown"].contains(&ext)
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// A JSON-tömb szöveges elemei (más alaknál üres).
fn strings(value: Option<&serde_json::Value>) -> impl Iterator<Item = String> + '_ {
    value
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
}

/// `packages/*`, `./modules/**` → `packages`, `modules` (a minta fix eleje).
fn pattern_prefix(pattern: &str) -> String {
    let pattern = pattern.trim_start_matches("./");
    let fixed = pattern.split(['*', '?', '{', '[']).next().unwrap_or("");
    fixed.trim_end_matches('/').to_owned()
}

/// `facebook/graph-sdk` → `facebook`.
fn vendor_prefix(name: &str) -> String {
    name.split('/').next().unwrap_or(name).to_ascii_lowercase()
}

/// `@acme/ui` → `@acme`; scope nélkül `None`.
fn npm_scope(name: &str) -> Option<String> {
    name.starts_with('@')
        .then(|| name.split('/').next().unwrap_or(name).to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_and_scopes() {
        assert_eq!(pattern_prefix("packages/*"), "packages");
        assert_eq!(pattern_prefix("./modules/**"), "modules");
        assert_eq!(pattern_prefix("apps/web"), "apps/web");
        assert_eq!(vendor_prefix("Facebook/graph-sdk"), "facebook");
        assert_eq!(npm_scope("@acme/ui").as_deref(), Some("@acme"));
        assert_eq!(npm_scope("lodash"), None);
        for name in ["LICENSE", "License.md", "LICENSE-MIT", "COPYING"] {
            assert!(is_license_name(name), "{name}");
        }
        for name in ["License.php", "Licenses.ts", "license.json"] {
            assert!(!is_license_name(name), "{name}");
        }
    }

    #[test]
    fn finds_vendor_and_generated_folders() {
        let root = std::env::temp_dir().join(format!(
            "callgraph-vendor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let write = |rel: &str, text: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "composer.json",
            r#"{"name": "acme/shop", "replace": {"illuminate/support": "self.version"},
                "repositories": [{"type": "path", "url": "modules/*"}]}"#,
        );
        write(
            "src/Illuminate/Support/composer.json",
            r#"{"name": "illuminate/support"}"#,
        );
        write("src/Illuminate/Support/LICENSE.md", "MIT");
        write("app/Models/License.php", "<?php\nclass License {}\n");
        write(
            "package.json",
            r#"{"name": "shop", "workspaces": ["packages/*"]}"#,
        );
        write(
            "sn_monitoring/composer.json",
            r#"{"name": "yiisoft/yii", "type": "library"}"#,
        );
        write("tools/parser/LICENSE.md", "MIT");
        write("acme-lib/composer.json", r#"{"name": "acme/lib"}"#);
        write(
            "backend/composer.json",
            r#"{"name": "laravel/laravel", "type": "project"}"#,
        );
        write(
            "frontend/package.json",
            r#"{"name": "front", "private": true, "version": "1.0.0"}"#,
        );
        write(
            "widgets/chart/package.json",
            r#"{"name": "chart.js", "version": "2.9.4"}"#,
        );
        write("packages/ui/LICENSE", "MIT");
        write("modules/billing/LICENSE", "MIT");
        let mut skipper = Skipper::new(&root, true);
        let vendor = |s: &str| Some(Skip::Vendor(s.to_owned()));
        assert_eq!(
            skipper.skip("extensions/Facebook/src/GraphNode.php"),
            vendor("extensions/Facebook")
        );
        assert_eq!(skipper.skip("vendors/a.php"), vendor("vendors"));
        assert_eq!(
            skipper.skip("sn_monitoring/framework/Base.php"),
            vendor("sn_monitoring")
        );
        assert_eq!(
            skipper.skip("tools/parser/src/Lexer.php"),
            vendor("tools/parser")
        );
        assert_eq!(
            skipper.skip("widgets/chart/src/chart.js"),
            vendor("widgets/chart")
        );
        assert_eq!(
            skipper.skip("web/assets.min/js/app.js"),
            Some(Skip::Generated("web/assets.min".into()))
        );
        assert_eq!(
            skipper.skip("frontend/dist/app.js"),
            Some(Skip::Generated("frontend/dist".into()))
        );
        // a projekt saját kódja: azonos vendor-előtag, "project" típus, privát csomag, workspace, path-repository
        for own in [
            "acme-lib/src/A.php",
            "backend/app/Models/User.php",
            "frontend/src/main.js",
            "packages/ui/src/Button.vue",
            "modules/billing/Invoice.php",
            "lib/Util.php",
            "app/Services/Event/Planner.php",
            "app/Models/License.php",
            "src/Illuminate/Support/Str.php",
            "index.php",
        ] {
            assert_eq!(skipper.skip(own), None, "{own}");
        }
        let mut with_vendor = Skipper::new(&root, false);
        assert_eq!(
            with_vendor.skip("extensions/Facebook/src/GraphNode.php"),
            None
        );
        assert_eq!(
            with_vendor.skip("web/assets.min/js/app.js"),
            Some(Skip::Generated("web/assets.min".into()))
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
