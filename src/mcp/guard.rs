//! Útvonal-őr: az MCP-eszközök útjai a projekt-gyökéren belül maradnak (`..`, abszolút út, kifelé mutató symlink: elutasítva).

use std::path::{Component, Path, PathBuf};

/// Egy lekérdezés gyökere: a kanonikus gyökér-mappa és a kimenetben megjelenő címke (alias vagy az út).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRoot {
    root: PathBuf,
    label: String,
}

impl ProjectRoot {
    /// A gyökér kanonikus alakra hozva; nem létező vagy nem mappa gyökér hiba.
    pub(crate) fn new(root: &Path, label: String) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .ok()
            .filter(|p| p.is_dir())
            .ok_or_else(|| {
                crate::tf!(
                    "Project root is not a directory: {}",
                    "A projekt-gyökér nem mappa: {}",
                    root.display()
                )
            })?;
        Ok(Self { root, label })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    /// A kért utat a gyökérhez képesti, `show`-nak átadható relatív úttá alakítja.
    /// Üres út = a gyökér (`.`). Létező útnál a symlinkeket is feloldja, és a gyökéren kívüli célt elutasítja.
    pub(crate) fn resolve(&self, requested: &str) -> Result<String, String> {
        if requested.contains("::") {
            return Err(crate::t!(
                "The path must not contain '::' — pass the method in the 'method' parameter.",
                "Az út nem tartalmazhat '::'-t — a metódust a 'method' paraméterben add meg."
            )
            .into());
        }
        let requested = Path::new(requested);
        let joined = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.root.join(requested)
        };
        let outside = || {
            crate::tf!(
                "Path is outside the project root ({}): {}",
                "Az út a projekt-gyökéren kívül esik ({}): {}",
                self.label,
                requested.display()
            )
        };
        // a lexikális alak (a nem létező utakhoz is): a `..` nem léphet a gyökér fölé
        let normalized = normalize(&joined).ok_or_else(outside)?;
        let resolved = match normalized.canonicalize() {
            Ok(real) => real,
            Err(_) if normalized.starts_with(&self.root) => normalized,
            Err(_) => return Err(outside()),
        };
        let relative = resolved.strip_prefix(&self.root).map_err(|_| outside())?;
        let text = relative.to_string_lossy().into_owned();
        Ok(match text.as_str() {
            "" => ".".to_owned(),
            // a `-` kezdetű név ne kapcsolónak látsszon
            t if t.starts_with('-') => format!(".{}{t}", std::path::MAIN_SEPARATOR),
            _ => text,
        })
    }
}

/// `.` és `..` feloldása a fájlrendszer nélkül; `None`, ha a `..` a gyökérnél (a prefixnél) feljebb lépne.
fn normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() || out.as_os_str().is_empty() {
                    return None;
                }
            }
            Component::Normal(name) => out.push(name),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cg-mcp-guard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/sub")).unwrap();
        std::fs::write(dir.join("src/a.php"), "<?php\n").unwrap();
        dir
    }

    #[test]
    fn keeps_paths_inside_the_root() {
        let dir = temp_root("inside");
        let root = ProjectRoot::new(&dir, "p".into()).unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(root.resolve("").unwrap(), ".");
        assert_eq!(root.resolve(".").unwrap(), ".");
        assert_eq!(root.resolve("src/a.php").unwrap(), format!("src{sep}a.php"));
        assert_eq!(
            root.resolve("src/sub/../a.php").unwrap(),
            format!("src{sep}a.php")
        );
        let absolute = root.root().join("src").display().to_string();
        assert_eq!(root.resolve(&absolute).unwrap(), "src");
        // nem létező, de belső út: a show adja a „Nem található” hibát
        assert_eq!(root.resolve("nope.php").unwrap(), "nope.php");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_escaping_paths() {
        let dir = temp_root("escape");
        let root = ProjectRoot::new(&dir, "p".into()).unwrap();
        assert!(root.resolve("../x").is_err());
        assert!(root.resolve("src/../../x").is_err());
        assert!(root.resolve("src/a.php::m").is_err());
        let outside = std::env::temp_dir().display().to_string();
        assert!(root.resolve(&outside).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_pointing_outside() {
        let dir = temp_root("link");
        let target = std::env::temp_dir();
        std::os::unix::fs::symlink(&target, dir.join("out")).unwrap();
        std::os::unix::fs::symlink(dir.join("src"), dir.join("in")).unwrap();
        let root = ProjectRoot::new(&dir, "p".into()).unwrap();
        assert!(root.resolve("out").is_err());
        assert_eq!(root.resolve("in").unwrap(), "src");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn root_must_be_a_directory() {
        let dir = temp_root("root");
        assert!(ProjectRoot::new(&dir.join("src/a.php"), "p".into()).is_err());
        assert!(ProjectRoot::new(&dir.join("missing"), "p".into()).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
