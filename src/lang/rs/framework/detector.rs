//! Factory: a fájl mappájától felfelé az első `Cargo.toml` függőségei alapján a framework-stratégia.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::FrameworkStrategy;
use super::cargo_toml::dependency_names;
use super::known::{KNOWN_FRAMEWORKS, KnownFramework};

/// Könyvtáranként cache-elt framework-választó.
#[derive(Default)]
pub(crate) struct FrameworkDetector {
    cache: HashMap<PathBuf, Option<&'static KnownFramework>>,
}

impl FrameworkDetector {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A könyvtárhoz tartozó framework; ha nincs `Cargo.toml` vagy ismert függőség: `None`.
    pub(crate) fn detect(&mut self, dir: &Path) -> Option<&'static dyn FrameworkStrategy> {
        let found = *self.cache.entry(dir.to_path_buf()).or_insert_with(|| {
            find_cargo_toml(dir)
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|toml| from_manifest(&toml))
        });
        found.map(|f| f as &dyn FrameworkStrategy)
    }
}

fn find_cargo_toml(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir;
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

/// A manifest függőségei közül a legmagasabb prioritású ismert framework.
fn from_manifest(toml: &str) -> Option<&'static KnownFramework> {
    let deps = dependency_names(toml);
    KNOWN_FRAMEWORKS
        .iter()
        .find(|f| deps.iter().any(|d| d == f.package()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_framework_wins_over_runtime_and_serde() {
        let toml = "[dependencies]\nserde = \"1\"\ntokio = \"1\"\nactix-web = \"4\"\n";
        assert_eq!(from_manifest(toml).map(|f| f.name()), Some("actix-web"));
    }

    #[test]
    fn unknown_dependencies_give_none() {
        assert!(from_manifest("[dependencies]\nregex = \"1\"\n").is_none());
        assert!(from_manifest("not toml at all").is_none());
    }

    #[test]
    fn detects_from_directory_and_caches() {
        let dir = std::env::temp_dir().join(format!("callgraph-rs-fw-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[dependencies]\naxum = \"0.7\"\n").unwrap();
        let mut detector = FrameworkDetector::new();
        let name = detector.detect(&dir.join("src")).map(|f| f.name());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(name, Some("axum"));
        assert_eq!(
            detector.detect(&dir.join("src")).map(|f| f.name()),
            Some("axum")
        );
    }
}
