//! A projekt frameworkjei a manifest-fájlokból, elemzés nélkül (a név-alapú, gyors profilhoz): a PHP-é és a JS-é a
//! csomagtérképből (`crate::packages`: a gyökér, az első szintű almappák és a workspace-ek `composer.json` /
//! `package.json` függőségei), a Rusté a gyökér `Cargo.toml`-jából.

use std::path::Path;

use crate::packages::Packages;

/// `Cargo.toml` függőség → framework (a webes keretrendszerek; a többit a fájlok `framework` mezője adja).
const RUST_CRATES: &[&str] = &["axum", "actix-web", "rocket", "warp", "poem"];

/// A manifestekből felismert frameworkök (ismétlés nélkül).
pub(crate) fn manifest_frameworks(root: &Path, packages: &Packages) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        if !out.iter().any(|have| have == name) {
            out.push(name.to_owned());
        }
    };
    for name in packages.frameworks() {
        push(name);
    }
    if let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) {
        for name in rust_frameworks(&text) {
            push(name);
        }
    }
    out
}

/// A `package.json` (`dependencies`, `devDependencies`, `peerDependencies`) frameworkjei.
#[cfg(test)]
pub(crate) fn js_frameworks(json: &str) -> Vec<&'static str> {
    crate::packages::node_frameworks(json)
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
