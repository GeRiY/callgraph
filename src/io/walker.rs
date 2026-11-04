//! Fájlbejárás: a megadott fájlok / mappák (rekurzívan) fájljai a regisztrált nyelvek kiterjesztéseivel.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::lang::LanguageRegistry;

/// A regisztrált kiterjesztésű fájlok gyűjtése; a kihagyott mappanevek konfigurálhatók.
pub struct FileWalker {
    extensions: Vec<&'static str>,
    skip_dirs: Vec<String>,
}

impl FileWalker {
    /// A registry kiterjesztés-listájával, az alapértelmezett kihagyott mappákkal.
    pub fn for_registry(registry: &LanguageRegistry) -> Self {
        Self::for_extensions(registry.extensions())
    }

    pub fn for_extensions(extensions: Vec<&'static str>) -> Self {
        Self {
            extensions,
            skip_dirs: default_skip_dirs(),
        }
    }

    fn matches(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| self.extensions.contains(&x))
    }
}

fn default_skip_dirs() -> Vec<String> {
    [
        "vendor",
        "node_modules",
        ".git",
        "cache",
        "runtime",
        "storage",
        "tmp",
        ".idea",
        ".junie",
    ]
    .map(String::from)
    .to_vec()
}

impl FileWalker {
    /// Egyedi kihagyott mappanevek (az alapértelmezettek helyett).
    pub fn with_skip_dirs(mut self, skip_dirs: Vec<String>) -> Self {
        self.skip_dirs = skip_dirs;
        self
    }

    /// A bemenetek fájljai rendezve; a hozzáférhetetlen mappákról a `warn` kap üzenetet.
    pub fn collect(&self, inputs: &[PathBuf], warn: &mut dyn FnMut(String)) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut visited = HashSet::new();
        for input in inputs {
            let Ok(real) = std::fs::canonicalize(input) else {
                warn(format!("Nem található: {}", input.display()));
                continue;
            };
            if real.is_dir() {
                self.walk_dir(&real, &mut files, &mut visited, warn);
            } else if real.is_file() && self.matches(&real) {
                files.push(real);
            }
        }
        files
    }

    fn walk_dir(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
        visited: &mut HashSet<PathBuf>,
        warn: &mut dyn FnMut(String),
    ) {
        let Ok(real_dir) = std::fs::canonicalize(dir) else {
            return;
        };
        if !visited.insert(real_dir.clone()) {
            return;
        }
        let dir = real_dir.as_path();
        let ignored = self.ignore_dirs_for(dir);
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => return warn(format!("{}: nem olvasható mappa: {e}", dir.display())),
        };
        let paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        for path in paths {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if ignored.iter().any(|ignored| ignored == name) {
                continue;
            }
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            // PHP is_dir follows links; canonical path tracking prevents cycles and duplicates.
            if metadata.file_type().is_symlink() {
                let Ok(target) = std::fs::canonicalize(&path) else {
                    continue;
                };
                if target.is_dir() {
                    self.walk_dir(&target, files, visited, warn);
                } else if target.is_file() && self.matches(&target) {
                    files.push(target);
                }
            } else if metadata.is_dir() {
                self.walk_dir(&path, files, visited, warn);
            } else if self.matches(&path) {
                files.push(path);
            }
        }
    }

    fn ignore_dirs_for(&self, dir: &Path) -> Vec<String> {
        let mut ignored = self.skip_dirs.clone();
        // The reference starts at dirname($dir), then walks upward to the
        // nearest .gitignore. Patterns are compared as whole entry names.
        let mut project_root = dir.parent();
        while let Some(candidate) = project_root {
            let gitignore = candidate.join(".gitignore");
            if let Ok(contents) = std::fs::read_to_string(gitignore) {
                ignored.extend(contents.lines().filter_map(|line| {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') || !line.ends_with('/') {
                        return None;
                    }
                    let name = line.trim_end_matches('/');
                    (!name.starts_with('!')).then(|| name.to_owned())
                }));
                break;
            }
            project_root = candidate.parent();
        }
        ignored
    }
}

#[cfg(test)]
mod tests {
    use super::FileWalker;
    use crate::lang::LanguageRegistry;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("callgraph-walker-{id}"));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn skips_builtin_and_gitignored_dirs_in_filesystem_order() {
        let temp = TempDir::new();
        fs::write(temp.0.join(".gitignore"), "generated/\n# comment\n!keep/\n").unwrap();
        // a/generated: a gyökér .gitignore-ja az ALMAPPÁK bejegyzéseire hat (a referencia a mappa szülőjétől keres)
        for dir in ["z", "a", "vendor", "generated", "cache", "a/generated"] {
            fs::create_dir_all(temp.0.join(dir)).unwrap();
            fs::write(temp.0.join(dir).join("f.php"), "<?php").unwrap();
        }
        fs::write(temp.0.join("root.php"), "<?php").unwrap();

        let mut warnings = Vec::new();
        let files = FileWalker::for_registry(&LanguageRegistry::with_defaults())
            .collect(std::slice::from_ref(&temp.0), &mut |w| warnings.push(w));
        assert!(warnings.is_empty());
        // a bejárás a fájlrendszer saját sorrendjét követi (APFS ≠ ext4), ezért a tartalmat rendezve vetjük össze
        let mut names: Vec<_> = files
            .iter()
            .map(|path| path.strip_prefix(temp.0.canonicalize().unwrap()).unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            // paritás az ast_graph.php-val: a gyökér saját `generated/` bejegyzésére a gyökér .gitignore-ja még nem hat
            // (a keresés a gyökér szülőjétől indul), az `a/generated` viszont kimarad
            [
                PathBuf::from("a/f.php"),
                PathBuf::from("generated/f.php"),
                PathBuf::from("root.php"),
                PathBuf::from("z/f.php")
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinks_without_cycles_or_duplicate_directories() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new();
        fs::create_dir(temp.0.join("dir")).unwrap();
        fs::write(temp.0.join("dir/f.php"), "<?php").unwrap();
        symlink(&temp.0, temp.0.join("dir/loop")).unwrap();
        symlink(temp.0.join("dir"), temp.0.join("alias")).unwrap();

        let files = FileWalker::for_registry(&LanguageRegistry::with_defaults())
            .collect(std::slice::from_ref(&temp.0), &mut |_| {});
        assert_eq!(files, [temp.0.join("dir/f.php").canonicalize().unwrap()]);
    }
}
