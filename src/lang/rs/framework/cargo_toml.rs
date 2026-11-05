//! A `Cargo.toml` függőségneveinek kiolvasása (TOML-crate nélkül: csak a táblafejlécek és a kulcsok kellenek).

/// A függőség-táblák (`[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`,
/// `[workspace.dependencies]`, `[target.….dependencies]`, `[dependencies.<név>]`) kulcsai.
pub(super) fn dependency_names(toml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_deps = false;
    for raw in toml.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_start_matches('[').trim_end_matches(']').trim();
            in_deps = false;
            match dependency_table(header) {
                Some(None) => in_deps = true,
                Some(Some(name)) => names.push(name),
                None => {}
            }
            continue;
        }
        if in_deps && let Some((key, _)) = line.split_once('=') {
            let key = key.trim().trim_matches('"');
            let name = key.split('.').next().unwrap_or(key).trim();
            if !name.is_empty() {
                names.push(name.to_owned());
            }
        }
    }
    names
}

/// `Some(None)`: függőség-tábla; `Some(Some(név))`: egyetlen függőség táblája; `None`: más tábla.
fn dependency_table(header: &str) -> Option<Option<String>> {
    const TABLES: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];
    let parts: Vec<&str> = header.split('.').map(str::trim).collect();
    let position = parts.iter().position(|p| TABLES.contains(p))?;
    let prefix_ok = matches!(&parts[..position], [] | ["workspace"] | ["target", ..]);
    if !prefix_ok {
        return None;
    }
    match &parts[position + 1..] {
        [] => Some(None),
        [name] => Some(Some(name.trim_matches('"').to_owned())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dependency_tables() {
        let toml = r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
axum = "0.7"
serde = { version = "1", features = ["derive"] } # comment
tokio.workspace = true

[dev-dependencies]
"pretty_assertions" = "1"

[target.'cfg(unix)'.dependencies]
nix = "0.29"

[dependencies.sqlx]
version = "0.8"

[profile.release]
lto = true
"#;
        assert_eq!(
            dependency_names(toml),
            ["axum", "serde", "tokio", "pretty_assertions", "nix", "sqlx"]
        );
    }
}
