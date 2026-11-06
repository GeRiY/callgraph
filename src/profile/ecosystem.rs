//! A projekt frameworkjei a manifest-fájlokból, elemzés nélkül (a név-alapú, gyors profilhoz): a PHP-é a meglévő
//! composer-detektorral (`lang::php::framework`), a JS-é a `package.json` függőségeiből (a gyökérben és az első
//! szintű almappákban: monorepo-frontend), a Rusté a gyökér `Cargo.toml`-jából.

use std::path::Path;

/// `package.json` függőség → framework.
const JS_PACKAGES: &[(&str, &str)] = &[
    ("nuxt", "nuxt"),
    ("vue", "vue"),
    ("next", "next"),
    ("react", "react"),
    ("svelte", "svelte"),
    ("@angular/core", "angular"),
];

/// `Cargo.toml` függőség → framework (a webes keretrendszerek; a többit a fájlok `framework` mezője adja).
const RUST_CRATES: &[&str] = &["axum", "actix-web", "rocket", "warp", "poem"];

/// Mappák, amelyekben nem keresünk `package.json`-t.
const SKIP: &[&str] = &["node_modules", "vendor", ".git", ".callgraph"];

/// A manifestekből felismert frameworkök (ismétlés nélkül).
pub(crate) fn manifest_frameworks(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        if !out.iter().any(|have| have == name) {
            out.push(name.to_owned());
        }
    };
    if root.join("composer.json").is_file() {
        let strategy = crate::lang::php::framework::FrameworkDetector::new().detect(root);
        if let Some(name) = strategy.name() {
            push(name);
        }
    }
    for dir in package_dirs(root) {
        if let Ok(text) = std::fs::read_to_string(dir.join("package.json")) {
            for name in js_frameworks(&text) {
                push(name);
            }
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) {
        for name in rust_frameworks(&text) {
            push(name);
        }
    }
    out
}

/// A gyökér és az első szintű almappák, ahol `package.json` van.
fn package_dirs(root: &Path) -> Vec<std::path::PathBuf> {
    let mut dirs = vec![root.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut children: Vec<_> = entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.is_dir())
            .filter(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| !SKIP.contains(&n))
            })
            .filter(|path| path.join("package.json").is_file())
            .collect();
        children.sort();
        dirs.extend(children);
    }
    dirs
}

/// A `package.json` (`dependencies`, `devDependencies`, `peerDependencies`) frameworkjei.
pub(crate) fn js_frameworks(json: &str) -> Vec<&'static str> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    JS_PACKAGES
        .iter()
        .filter(|(package, _)| {
            ["dependencies", "devDependencies", "peerDependencies"]
                .iter()
                .any(|section| value.get(*section).and_then(|d| d.get(*package)).is_some())
        })
        .map(|(_, name)| *name)
        .collect()
}

/// A `Cargo.toml` webes frameworkjei: a sor eleji `név =` / `[dependencies.név]` alak.
pub(crate) fn rust_frameworks(toml: &str) -> Vec<&'static str> {
    RUST_CRATES
        .iter()
        .copied()
        .filter(|name| {
            toml.lines().map(str::trim_start).any(|line| {
                line.strip_prefix(name).is_some_and(|rest| {
                    let rest = rest.trim_start();
                    rest.starts_with('=') || rest.starts_with('.')
                }) || line == format!("[dependencies.{name}]")
            })
        })
        .collect()
}
