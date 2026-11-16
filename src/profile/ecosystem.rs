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

/// A `Cargo.toml` `[workspace]` `members` listája, ahogy le van írva (a `crates/*`-hoz hasonló mintákkal).
pub(crate) fn workspace_members(toml: &str) -> Vec<String> {
    let mut in_workspace = false;
    let mut collecting = false;
    let mut out: Vec<String> = Vec::new();
    for line in toml.lines().map(str::trim) {
        if collecting {
            out.extend(quoted(line));
            collecting = !line.contains(']');
            continue;
        }
        if line.starts_with('[') {
            in_workspace = line == "[workspace]";
            continue;
        }
        if !in_workspace {
            continue;
        }
        let Some(rest) = line
            .strip_prefix("members")
            .map(str::trim_start)
            .and_then(|rest| rest.strip_prefix('='))
            .map(str::trim_start)
        else {
            continue;
        };
        out.extend(quoted(rest));
        collecting = rest.starts_with('[') && !rest.contains(']');
    }
    out
}

/// A sor idézőjeles szövegei (a `#` megjegyzés előtt).
fn quoted(line: &str) -> Vec<String> {
    let line = line.split('#').next().unwrap_or(line);
    line.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// A workspace tagjai a gyökérből: a `members` minták (`crates/*` → a `Cargo.toml`-os almappák) kibontva; ha nincs
/// `[workspace]`, de van `crates/` mappa `Cargo.toml`-os almappákkal, azok. Üres, ha egyetlen crate.
pub(crate) fn rust_crates(root: &Path) -> Vec<String> {
    let mut patterns = std::fs::read_to_string(root.join("Cargo.toml"))
        .map(|text| workspace_members(&text))
        .unwrap_or_default();
    if patterns.is_empty() {
        patterns.push("crates/*".to_owned());
    }
    let mut out: Vec<String> = Vec::new();
    for pattern in patterns {
        match pattern.strip_suffix("/*") {
            Some(dir) => {
                let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.join("Cargo.toml").is_file()
                        && let Some(name) = path.file_name().and_then(|n| n.to_str())
                    {
                        out.push(format!("{dir}/{name}"));
                    }
                }
            }
            None if root.join(&pattern).join("Cargo.toml").is_file() => out.push(pattern),
            None => {}
        }
    }
    out.sort();
    out.dedup();
    out
}
