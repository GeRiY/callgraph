//! A csomagtérkép egységtesztjei memóriabeli fájlrendszeren (`SourceFiles`) és cache-tárolón (`CacheStore`): a
//! builder (lock vs. tartomány, dev, tranzitív), a Composite (monorepo, `nearest`), a cache (bélyeg), a stack-sor és a
//! `--packages` lista.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::cache::{CacheStore, load_or_build};
use super::files::{SourceFiles, Stamp};
use super::model::{Ecosystem, Package};
use super::project::{EcosystemPackages, ProjectPackages};
use super::{Packages, composer_requires};
use crate::i18n::Lang;

/// Memóriabeli fájlrendszer: út → (szöveg, bélyeg); az olvasásokat számolja.
#[derive(Default)]
struct MemFiles {
    files: RefCell<BTreeMap<PathBuf, (String, Stamp)>>,
    reads: RefCell<Vec<PathBuf>>,
}

impl MemFiles {
    fn new(files: &[(&str, &str)]) -> Self {
        let mem = Self::default();
        for (rel, text) in files {
            mem.set(rel, text, 1);
        }
        mem
    }

    /// Egy fájl (újra)írása a megadott „mtime”-mal.
    fn set(&self, rel: &str, text: &str, mtime: u64) {
        let stamp = Stamp {
            mtime,
            size: text.len() as u64,
            ctime: mtime,
        };
        self.files
            .borrow_mut()
            .insert(root().join(rel), (text.to_owned(), stamp));
    }

    /// Hányszor olvasták a gyökérhez relatív fájlt.
    fn reads_of(&self, rel: &str) -> usize {
        let path = root().join(rel);
        self.reads.borrow().iter().filter(|p| **p == path).count()
    }
}

impl SourceFiles for MemFiles {
    fn read(&self, path: &Path) -> Option<String> {
        self.reads.borrow_mut().push(path.to_path_buf());
        self.files.borrow().get(path).map(|(text, _)| text.clone())
    }

    fn stamp(&self, path: &Path) -> Option<Stamp> {
        self.files.borrow().get(path).map(|(_, stamp)| *stamp)
    }

    fn dirs(&self, dir: &Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .files
            .borrow()
            .keys()
            .filter_map(|path| {
                let mut parts = path.strip_prefix(dir).ok()?.components();
                let first = parts.next()?;
                parts.next()?;
                Some(dir.join(first))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// Memóriabeli cache-tároló.
#[derive(Default)]
struct MemStore(RefCell<Option<Vec<u8>>>);

impl CacheStore for MemStore {
    fn load(&self) -> Option<Vec<u8>> {
        self.0.borrow().clone()
    }

    fn save(&self, bytes: &[u8]) {
        *self.0.borrow_mut() = Some(bytes.to_vec());
    }
}

fn root() -> PathBuf {
    PathBuf::from("/proj")
}

/// Felépítés cache nélkül (minden hívás új, üres tárolóval).
fn build(files: &MemFiles) -> ProjectPackages {
    load_or_build(&root(), files, &MemStore::default())
}

const COMPOSER_JSON: &str = r#"{
    "name": "acme/shop",
    "require": {"php": "^8.2", "laravel/framework": "^11.0", "livewire/livewire": "^3.4"},
    "require-dev": {"phpunit/phpunit": "^10.5"},
    "autoload": {"psr-4": {"App\\": "app/"}}
}"#;

const COMPOSER_LOCK: &str = r#"{
    "packages": [
        {"name": "laravel/framework", "version": "v11.31.0", "autoload": {"psr-4": {"Illuminate\\": "src/Illuminate/"}}},
        {"name": "livewire/livewire", "version": "v3.5.2", "autoload": {"psr-4": {"Livewire\\": "src/"}}},
        {"name": "symfony/console", "version": "v7.1.0", "autoload": {"psr-4": {"Symfony\\Component\\Console\\": ""}}}
    ],
    "packages-dev": [
        {"name": "phpunit/phpunit", "version": "10.5.1"}
    ]
}"#;

fn package<'a>(part: &'a EcosystemPackages, name: &str) -> &'a Package {
    part.packages
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("{name}: {part:?}"))
}

#[test]
fn builder_lock_overrides_the_range_and_keeps_transitive_apart() {
    let files = MemFiles::new(&[
        ("composer.json", COMPOSER_JSON),
        ("composer.lock", COMPOSER_LOCK),
    ]);
    let map = build(&files);
    assert_eq!(map.parts.len(), 1);
    let part = &map.parts[0];
    assert_eq!(part.ecosystem, Ecosystem::Php);
    assert_eq!(part.root, root());
    assert_eq!(part.name.as_deref(), Some("acme/shop"));
    assert_eq!(part.lock.as_deref(), Some("composer.lock"));
    assert_eq!(part.own_namespaces, ["App\\"]);
    assert_eq!(
        package(part, "laravel/framework"),
        &Package {
            name: "laravel/framework".into(),
            ecosystem: Ecosystem::Php,
            declared: Some("^11.0".into()),
            installed: Some("11.31.0".into()),
            dev: false,
            namespaces: vec!["Illuminate\\".into()],
            auto_discovered: false,
            plugin: false,
        }
    );
    let phpunit = package(part, "phpunit/phpunit");
    assert!(phpunit.dev);
    assert_eq!(phpunit.installed.as_deref(), Some("10.5.1"));
    let direct: Vec<&str> = part.packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        direct,
        ["laravel/framework", "livewire/livewire", "phpunit/phpunit"]
    );
    let transitive: Vec<&str> = part
        .installed_only
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(transitive, ["symfony/console"]);
    assert_eq!(part.installed_only[0].declared, None);
    assert_eq!(
        part.installed_only[0].namespaces,
        ["Symfony\\Component\\Console\\"]
    );
}

#[test]
fn builder_without_lock_keeps_the_range_and_marks_it() {
    let files = MemFiles::new(&[(
        "package.json",
        r#"{"dependencies": {"vue": "^3.4"}, "devDependencies": {"vite": "^5.0"}}"#,
    )]);
    let map = build(&files);
    let part = &map.parts[0];
    assert_eq!(part.ecosystem, Ecosystem::Node);
    assert_eq!(part.lock, None);
    assert_eq!(package(part, "vue").installed, None);
    assert_eq!(package(part, "vue").declared.as_deref(), Some("^3.4"));
    assert!(package(part, "vite").dev);
    assert!(part.installed_only.is_empty());
    let packages = Packages::from_map(map);
    assert_eq!(
        packages.stack().line(Lang::En),
        "stack:   vue ^3.4 (range) · vite ^5.0 (range)"
    );
    assert_eq!(
        packages.stack().line(Lang::Hu),
        "stack:   vue ^3.4 (tartomány) · vite ^5.0 (tartomány)"
    );
}

#[test]
fn builder_broken_manifest_is_an_empty_part_and_broken_lock_means_ranges() {
    let files = MemFiles::new(&[
        ("composer.json", "{ not json"),
        ("package.json", r#"{"dependencies": {"vue": "^3.4"}}"#),
        ("package-lock.json", "{ broken"),
        ("yarn.lock", "vue@^3.4:\n  version \"3.4.21\"\n"),
    ]);
    let map = build(&files);
    assert_eq!(map.parts.len(), 2);
    assert!(map.parts[0].packages.is_empty());
    // az első létező lock nyer (package-lock.json), akkor is, ha hibás: nincs telepített verzió
    let node = &map.parts[1];
    assert_eq!(node.lock, None);
    assert_eq!(package(node, "vue").installed, None);
}

#[test]
fn builder_picks_the_yarn_version_resolved_for_the_declared_range() {
    let files = MemFiles::new(&[
        ("package.json", r#"{"dependencies": {"vue": "^3.4.0"}}"#),
        (
            "yarn.lock",
            "vue@^2.6.0:\n  version \"2.6.14\"\n\nvue@^3.4.0:\n  version \"3.4.21\"\n",
        ),
    ]);
    let map = build(&files);
    let part = &map.parts[0];
    assert_eq!(part.lock.as_deref(), Some("yarn.lock"));
    assert_eq!(package(part, "vue").installed.as_deref(), Some("3.4.21"));
}

/// Monorepo: PHP-backend, Vue-frontend, a gyökér npm-workspace-szel és a gyökér lockjával.
fn monorepo() -> MemFiles {
    MemFiles::new(&[
        (
            "package.json",
            r#"{"name": "mono", "private": true, "workspaces": ["packages/*"],
                "devDependencies": {"typescript": "~5.3.0"}}"#,
        ),
        (
            "package-lock.json",
            r#"{"lockfileVersion": 3, "packages": {
                "": {"name": "mono"},
                "node_modules/typescript": {"version": "5.3.3"},
                "node_modules/vue": {"version": "3.4.21"},
                "node_modules/@acme/ui": {"resolved": "packages/ui", "link": true}
            }}"#,
        ),
        ("backend/composer.json", COMPOSER_JSON),
        ("backend/composer.lock", COMPOSER_LOCK),
        ("backend/app/Models/User.php", "<?php\n"),
        (
            "frontend/package.json",
            r#"{"dependencies": {"vue": "^3.3", "pinia": "^2.1"}}"#,
        ),
        (
            "packages/ui/package.json",
            r#"{"name": "@acme/ui", "dependencies": {"vue": "^3.4"}}"#,
        ),
        ("node_modules/vue/package.json", r#"{"name": "vue"}"#),
        ("vendor/acme/lib/composer.json", r#"{"name": "acme/lib"}"#),
        ("docs/readme.md", "# docs\n"),
    ])
}

#[test]
fn composite_monorepo_parts_and_nearest() {
    let map = build(&monorepo());
    let parts: Vec<(PathBuf, Ecosystem)> = map
        .parts
        .iter()
        .map(|part| (part.root.clone(), part.ecosystem))
        .collect();
    assert_eq!(
        parts,
        [
            (root(), Ecosystem::Node),
            (root().join("backend"), Ecosystem::Php),
            (root().join("frontend"), Ecosystem::Node),
            (root().join("packages/ui"), Ecosystem::Node),
        ]
    );
    // a workspace-csomag a gyökér lockját használja; a frontend (nem workspace) lock nélkül marad
    let ui = &map.parts[3];
    assert_eq!(ui.lock.as_deref(), Some("package-lock.json"));
    assert_eq!(package(ui, "vue").installed.as_deref(), Some("3.4.21"));
    let frontend = &map.parts[2];
    assert_eq!(frontend.lock, None);
    assert_eq!(package(frontend, "vue").installed, None);

    let nearest = |rel: &str, ecosystem| {
        map.nearest(&root().join(rel), ecosystem)
            .map(|p| p.root.clone())
    };
    assert_eq!(
        nearest("packages/ui/src/Button.vue", Ecosystem::Node),
        Some(root().join("packages/ui"))
    );
    assert_eq!(
        nearest("backend/app/Models/User.php", Ecosystem::Php),
        Some(root().join("backend"))
    );
    assert_eq!(nearest("scripts/build.js", Ecosystem::Node), Some(root()));
    assert_eq!(nearest("frontend/src/main.ts", Ecosystem::Php), None);
    assert_eq!(map.all().count(), 4);

    let packages = Packages::from_map(map);
    assert_eq!(packages.frameworks(), ["laravel", "vue"]);
    assert_eq!(
        packages.stack().line(Lang::En),
        "stack:   laravel/framework 11.31.0 · livewire/livewire 3.5.2 · phpunit/phpunit 10.5.1 · vue ^3.3 (range) · pinia ^2.1 (range) · typescript 5.3.3   (package-lock.json, composer.lock)"
    );
    assert_eq!(packages.all_direct().len(), 1 + 3 + 2 + 1);
    assert!(!packages.is_empty());
}

#[test]
fn cache_is_reused_until_a_lock_changes() {
    let files = MemFiles::new(&[
        ("composer.json", COMPOSER_JSON),
        ("composer.lock", COMPOSER_LOCK),
    ]);
    let store = MemStore::default();
    let first = load_or_build(&root(), &files, &store);
    assert_eq!(files.reads_of("composer.lock"), 1);
    assert!(store.0.borrow().is_some(), "the map is saved");

    let again = load_or_build(&root(), &files, &store);
    assert_eq!(again, first);
    assert_eq!(files.reads_of("composer.lock"), 1, "served from the cache");

    files.set(
        "composer.lock",
        &COMPOSER_LOCK.replace("v11.31.0", "v11.32.1"),
        2,
    );
    let changed = load_or_build(&root(), &files, &store);
    assert_eq!(
        files.reads_of("composer.lock"),
        2,
        "rebuilt after the stamp changed"
    );
    assert_eq!(
        package(&changed.parts[0], "laravel/framework")
            .installed
            .as_deref(),
        Some("11.32.1")
    );

    // egy új szóba jöhető fájl (almappa manifestje) is újraépít
    files.set(
        "frontend/package.json",
        r#"{"dependencies": {"vue": "^3.4"}}"#,
        3,
    );
    let grown = load_or_build(&root(), &files, &store);
    assert_eq!(grown.parts.len(), 2);
    assert_eq!(files.reads_of("composer.lock"), 3);

    // sérült cache: csendben újraépül
    *store.0.borrow_mut() = Some(vec![1, 2, 3]);
    assert_eq!(load_or_build(&root(), &files, &store), grown);
}

/// Egy rész a megadott (név, verzió) csomagokkal, lockkal.
fn locked(ecosystem: Ecosystem, lock: &str, names: &[(&str, &str)]) -> EcosystemPackages {
    EcosystemPackages {
        root: root(),
        ecosystem,
        name: None,
        packages: names
            .iter()
            .map(|(name, version)| Package {
                name: (*name).to_owned(),
                ecosystem,
                declared: Some(format!("^{version}")),
                installed: Some((*version).to_owned()),
                dev: false,
                namespaces: Vec::new(),
                auto_discovered: false,
                plugin: false,
            })
            .collect(),
        installed_only: Vec::new(),
        own_namespaces: Vec::new(),
        lock: Some(lock.to_owned()),
    }
}

#[test]
fn stack_keeps_the_fixed_order_and_caps_at_ten() {
    let php = locked(
        Ecosystem::Php,
        "composer.lock",
        &[
            ("guzzlehttp/guzzle", "7.9.2"),
            ("phpunit/phpunit", "10.5.1"),
            ("livewire/livewire", "3.5.2"),
            ("laravel/framework", "11.31.0"),
        ],
    );
    let node = locked(
        Ecosystem::Node,
        "pnpm-lock.yaml",
        &[
            ("jest", "29.7.0"),
            ("vite", "5.1.4"),
            ("typescript", "5.3.3"),
            ("tailwindcss", "3.4.1"),
            ("pinia", "2.1.7"),
            ("@inertiajs/vue3", "1.0.14"),
            ("vue", "3.4.21"),
            ("axios", "1.6.7"),
            ("vitest", "1.3.1"),
        ],
    );
    let map = ProjectPackages {
        root: root(),
        parts: vec![php, node],
    };
    let stack = Packages::from_map(map).stack();
    let names: Vec<&str> = stack.entries.iter().map(|e| e.name).collect();
    assert_eq!(
        names,
        [
            "laravel/framework",
            "livewire/livewire",
            "phpunit/phpunit",
            "vue",
            "@inertiajs/vue3",
            "pinia",
            "tailwindcss",
            "typescript",
            "vite",
            "vitest",
            "jest"
        ]
    );
    assert_eq!(stack.locks, ["composer.lock", "pnpm-lock.yaml"]);
    let line = stack.line(Lang::En);
    assert!(
        line.starts_with("stack:   laravel/framework 11.31.0 · livewire/livewire 3.5.2 · phpunit/phpunit 10.5.1 · vue 3.4.21 · "),
        "{line}"
    );
    assert!(
        line.ends_with(" · vitest 1.3.1 · +1   (composer.lock, pnpm-lock.yaml)"),
        "{line}"
    );
    assert!(!line.contains("jest"), "{line}");
    assert!(
        !line.contains("guzzle") && !line.contains("axios"),
        "{line}"
    );
    assert_eq!(
        stack.line(Lang::Hu),
        line,
        "only the range mark is translated"
    );
    assert!(
        Packages::from_map(ProjectPackages::default())
            .stack()
            .is_empty()
    );
}

#[test]
fn listing_prints_prod_then_dev_per_part() {
    let packages = Packages::from_map(build(&monorepo()));
    let en = packages.listing(Lang::En);
    let expected = "packages [node] . (mono) · package-lock.json
  typescript  5.3.3 (~5.3.0)  [dev]
packages [php] backend (acme/shop) · composer.lock
  laravel/framework  11.31.0 (^11.0)
  livewire/livewire  3.5.2 (^3.4)
  phpunit/phpunit    10.5.1 (^10.5)  [dev]
packages [node] frontend · no lock file (declared ranges only)
  pinia  (^2.1)
  vue    (^3.3)
packages [node] packages/ui (@acme/ui) · package-lock.json
  vue  3.4.21 (^3.4)
";
    assert_eq!(en, expected);
    let hu = packages.listing(Lang::Hu);
    assert!(
        hu.contains("csomagok [node] frontend · nincs lock-fájl (csak a deklarált tartományok)\n"),
        "{hu}"
    );
    let empty = Packages::from_map(ProjectPackages::default());
    assert!(empty.is_empty());
    assert!(
        empty
            .listing(Lang::En)
            .starts_with("packages: no dependencies found"),
        "{}",
        empty.listing(Lang::En)
    );
}

#[test]
fn composer_requires_keeps_the_dev_flag_and_skips_the_platform() {
    assert_eq!(
        composer_requires(COMPOSER_JSON),
        [
            ("laravel/framework".to_owned(), false),
            ("livewire/livewire".to_owned(), false),
            ("phpunit/phpunit".to_owned(), true)
        ]
    );
    assert!(composer_requires("nope").is_empty());
}

/// A tömör (MCP-indító) stack-sor: rövid nevek, főverzió, a teszt- és build-eszközök nélkül.
#[test]
fn compact_stack_line_has_short_names_and_major_versions() {
    use super::stack::{Stack, StackEntry};
    let entry = |name: &'static str, version: &str, from_lock: bool| StackEntry {
        name,
        version: Some(version.to_owned()),
        from_lock,
    };
    let stack = Stack {
        entries: vec![
            entry("laravel/framework", "12.69.3", true),
            entry("inertiajs/inertia-laravel", "v3.4.0", true),
            entry("phpunit/phpunit", "11.5.56", true),
            entry("vue", "^3.5.0", false),
            entry("@inertiajs/vue3", "2.3.27", true),
            entry("tailwindcss", "4.3.3", true),
            entry("typescript", "5.9.3", true),
            entry("vite", "8.2.2", true),
            entry("pinia", "0.9.1", true),
        ],
        locks: vec!["composer.lock".into()],
    };
    assert_eq!(
        stack.compact_line().as_deref(),
        Some("stack: laravel 12 · inertia 3 · vue 3 · inertia-vue 2 · tailwindcss 4 · pinia 0.9")
    );
    assert_eq!(Stack::default().compact_line(), None);
    let tools_only = Stack {
        entries: vec![entry("vite", "8.0.0", true)],
        locks: Vec::new(),
    };
    assert_eq!(tools_only.compact_line(), None);
}
