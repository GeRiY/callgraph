//! `Project`: egy projekt-gyökér és azonosítója.

use std::path::{Path, PathBuf};

/// Egy projekt: a gyökér-út, az azonosító (a gyökér mappaneve) és egy opcionális alias.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Project {
    root: PathBuf,
    alias: Option<String>,
}

impl Project {
    /// Projekt a megadott gyökérrel, alias nélkül.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            alias: None,
        }
    }

    /// Ugyanez a projekt a megadott aliassal.
    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    /// A projekt-gyökér útja.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Azonosító: a gyökér mappaneve (a fájlrendszer gyökerénél `root`).
    pub fn id(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "root".to_string())
    }

    /// A callgraph `projects.json`-ból származó alias, ha van.
    pub fn alias(&self) -> Option<&str> {
        self.alias.as_deref()
    }

    /// Megjelenítési név: az alias, ennek hiányában az azonosító.
    pub fn display_name(&self) -> String {
        self.alias.clone().unwrap_or_else(|| self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_root_dir_name() {
        let p = Project::new("/a/b/myapp");
        assert_eq!(p.id(), "myapp");
        assert_eq!(p.alias(), None);
        assert_eq!(p.display_name(), "myapp");
    }

    #[test]
    fn alias_overrides_display_name_only() {
        let p = Project::new("/a/b/x").with_alias("gw");
        assert_eq!(p.id(), "x");
        assert_eq!(p.display_name(), "gw");
    }

    #[test]
    fn filesystem_root_has_fallback_id() {
        assert_eq!(Project::new("/").id(), "root");
    }
}
