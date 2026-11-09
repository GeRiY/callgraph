//! Fájlbejárás: a megadott fájlok / mappák (rekurzívan) fájljai a regisztrált nyelvek kiterjesztéseivel
//! (kis-/nagybetűtől függetlenül), a generált fájlok (`lang::filetype::is_generated`), a beépített kihagyott
//! mappák, a `.gitignore` és a `.callgraphignore` szabályai (`ignore_rules`) nélkül; minden fájl egyszer (a
//! symlinkek célja is). A `collect`, az `includes` és az `includes_dir` ugyanazokkal a szabályokkal dönt (a bejárás,
//! az MCP-figyelő, a hook és a `refresh` így egyetért).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::ignore_rules::{IgnoreTree, excluded_extensions, gitignore_enabled};
use crate::lang::LanguageRegistry;

/// A regisztrált kiterjesztésű fájlok gyűjtése; a kihagyott mappanevek konfigurálhatók.
pub struct FileWalker {
    extensions: Vec<&'static str>,
    skip_dirs: Vec<String>,
    /// A `.gitignore` fájlok is számítanak-e (alapból a `CALLGRAPH_GITIGNORE` szerint, `--no-gitignore`).
    gitignore: bool,
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
            gitignore: gitignore_enabled(),
        }
    }

    /// A `.gitignore` fájlok figyelembe vétele (alapból a `CALLGRAPH_GITIGNORE` környezeti változó szerint).
    pub fn with_gitignore(mut self, gitignore: bool) -> Self {
        self.gitignore = gitignore;
        self
    }

    /// Feldolgozandó-e a fájl: regisztrált kiterjesztés (kis-/nagybetűtől függetlenül), nem generált fájl, és a
    /// legközelebbi `.callgraphignore` nem zárja ki a kiterjesztését.
    fn matches(&self, path: &Path) -> bool {
        let excluded = path.parent().map(excluded_extensions).unwrap_or_default();
        self.matches_in(path, &excluded)
    }

    /// Mint a `matches`, a mappára érvényes kizárt kiterjesztésekkel (a bejárás mappánként egyszer olvassa be).
    fn matches_in(&self, path: &Path, excluded: &[String]) -> bool {
        let Some(ext) = crate::lang::filetype::extension_lower(path) else {
            return false;
        };
        self.extensions.contains(&ext.as_str())
            && !crate::lang::filetype::is_generated(path)
            && !excluded.contains(&ext)
    }

    /// A `.callgraphignore` kizárja-e a fájl (regisztrált) kiterjesztését — a közvetlenül megadott fájl hibaüzenetéhez.
    pub fn excluded_by_ignore_file(&self, path: &Path) -> bool {
        crate::lang::filetype::extension_lower(path).is_some_and(|ext| {
            path.parent()
                .is_some_and(|dir| excluded_extensions(dir).contains(&ext))
        })
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

    /// A bemenetek fájljai rendezve; a hozzáférhetetlen mappákról a `warn` kap üzenetet. A közvetlenül megadott
    /// fájl a `.gitignore`-tól függetlenül bekerül (a felhasználó maga kérte).
    pub fn collect(&self, inputs: &[PathBuf], warn: &mut dyn FnMut(String)) -> Vec<PathBuf> {
        let mut files = self.collect_all(inputs, warn);
        // minden fájl egyszer: egy symlink (`out/link.php → src/A.php`) célja a kanonikus útjával kerül be,
        // ahogy maga a fájl is — az első előfordulás marad
        let mut seen = HashSet::new();
        files.retain(|file| seen.insert(file.clone()));
        files
    }

    fn collect_all(&self, inputs: &[PathBuf], warn: &mut dyn FnMut(String)) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut visited = HashSet::new();
        let mut rules = IgnoreTree::new(self.gitignore);
        for input in inputs {
            let Ok(real) = std::fs::canonicalize(input) else {
                warn(crate::tf!(
                    "Not found: {}",
                    "Nem található: {}",
                    input.display()
                ));
                continue;
            };
            if real.is_dir() {
                self.walk_dir(&real, &mut files, &mut visited, &mut rules, warn);
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
        rules: &mut IgnoreTree,
        warn: &mut dyn FnMut(String),
    ) {
        let Ok(real_dir) = std::fs::canonicalize(dir) else {
            return;
        };
        if !visited.insert(real_dir.clone()) {
            return;
        }
        let dir = real_dir.as_path();
        let skipped = self.skip_names_for(dir);
        let excluded = excluded_extensions(dir);
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                return warn(crate::tf!(
                    "{}: directory cannot be read: {e}",
                    "{}: nem olvasható mappa: {e}",
                    dir.display()
                ));
            }
        };
        let paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        for path in paths {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if skipped.iter().any(|skipped| skipped == name) {
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
                // a minta a link saját útjára illeszkedik (mint a gitnél)
                if rules.ignored(dir, &path, target.is_dir()) {
                    continue;
                }
                if target.is_dir() {
                    self.walk_dir(&target, files, visited, rules, warn);
                } else if target.is_file() && self.matches(&target) {
                    files.push(target);
                }
            } else if metadata.is_dir() {
                if !rules.ignored(dir, &path, true) {
                    self.walk_dir(&path, files, visited, rules, warn);
                }
            } else if self.matches_in(&path, &excluded) && !rules.ignored(dir, &path, false) {
                files.push(path);
            }
        }
    }

    /// Bejárná-e a `collect(&[root])` ezt a fájlt: a kiterjesztése regisztrált, a gyökér alatti egyik mappája sincs
    /// kihagyva (`includes_dir`), és a `.gitignore` / `.callgraphignore` mintái sem zárják ki. A fájlnak nem kell
    /// léteznie; a `root` és a `file` ugyanabban az alakban (kanonikus út). Symlinkeket nem követ (a `callgraph
    /// refresh` / az MCP-figyelő szűrője).
    pub fn includes(&self, root: &Path, file: &Path) -> bool {
        let Some(dir) = file.parent() else {
            return false;
        };
        let mut rules = IgnoreTree::new(self.gitignore);
        self.matches(file)
            && self.includes_dir_with(root, dir, &mut rules)
            && !rules.ignored(dir, file, false)
    }

    /// Bejárná-e a `collect(&[root])` ezt a mappát: a gyökér alatt van, és egyik szintje sincs kihagyva (beépített
    /// név, Cargo `target/`, `.gitignore`- vagy `.callgraphignore`-minta). A gyökér maga benne van.
    pub fn includes_dir(&self, root: &Path, dir: &Path) -> bool {
        self.includes_dir_with(root, dir, &mut IgnoreTree::new(self.gitignore))
    }

    fn includes_dir_with(&self, root: &Path, dir: &Path, rules: &mut IgnoreTree) -> bool {
        let Ok(rel) = dir.strip_prefix(root) else {
            return false;
        };
        let mut at = root.to_path_buf();
        for component in rel.components() {
            let std::path::Component::Normal(name) = component else {
                return false;
            };
            let Some(name) = name.to_str() else {
                return false;
            };
            if self.skip_names_for(&at).iter().any(|skipped| skipped == name) {
                return false;
            }
            let child = at.join(name);
            if rules.ignored(&at, &child, true) {
                return false;
            }
            at = child;
        }
        true
    }

    /// A mappa név szerint kihagyott bejegyzései: a beépített nevek, és Cargo-csomagban a `target/`.
    fn skip_names_for(&self, dir: &Path) -> Vec<String> {
        let mut skipped = self.skip_dirs.clone();
        // Cargo-csomag: a `target/` build-kimenet (generált .rs fájlokkal). Csak Cargo.toml mellett,
        // hogy a PHP / JS projektek `target` nevű mappáit ne érintse.
        if dir.join("Cargo.toml").is_file() {
            skipped.push("target".to_owned());
        }
        skipped
    }
}

#[cfg(test)]
mod tests {
    use super::FileWalker;
    use crate::io::ignore_rules::{IGNORE_FILE, excluded_extensions, parse_ignore, path_patterns};
    use crate::lang::LanguageRegistry;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "callgraph-walker-{id}-{}-{}",
                std::process::id(),
                {
                    static N: std::sync::atomic::AtomicUsize =
                        std::sync::atomic::AtomicUsize::new(0);
                    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                }
            ));
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
        // git-szemantika: a gyökér `generated/` mintája minden szinten hat (a gyökér saját `generated/`-jére is)
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
            // valódi .gitignore-szemantika (a régi ast_graph.php-paritás helyett: ott a gyökér saját `generated/`-je
            // bekerült, mert a keresés a gyökér szülőjétől indult)
            [
                PathBuf::from("a/f.php"),
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

    #[test]
    fn skips_cargo_target_only_next_to_cargo_toml() {
        let temp = TempDir::new();
        // Cargo-csomag: a target/ kimarad
        fs::create_dir_all(temp.0.join("crate/src")).unwrap();
        fs::create_dir_all(temp.0.join("crate/target/debug/build")).unwrap();
        fs::write(temp.0.join("crate/Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(temp.0.join("crate/src/lib.rs"), "pub fn a() {}").unwrap();
        fs::write(temp.0.join("crate/target/debug/build/out.rs"), "fn g() {}").unwrap();
        // nem Cargo-projekt: a target/ marad
        fs::create_dir_all(temp.0.join("web/target")).unwrap();
        fs::write(temp.0.join("web/target/page.php"), "<?php").unwrap();

        let root = temp.0.canonicalize().unwrap();
        let mut files: Vec<PathBuf> = FileWalker::for_registry(&LanguageRegistry::with_defaults())
            .collect(std::slice::from_ref(&temp.0), &mut |_| {})
            .into_iter()
            .map(|f| f.strip_prefix(&root).unwrap().to_path_buf())
            .collect();
        files.sort();
        assert_eq!(
            files,
            [
                PathBuf::from("crate/src/lib.rs"),
                PathBuf::from("web/target/page.php")
            ]
        );
    }

    #[test]
    fn includes_matches_what_collect_would_walk() {
        let temp = TempDir::new();
        let root = temp.0.canonicalize().unwrap();
        fs::write(root.join(".gitignore"), "build/\n").unwrap();
        fs::create_dir_all(root.join("crate")).unwrap();
        fs::write(root.join("crate/Cargo.toml"), "[package]\n").unwrap();
        let walker = FileWalker::for_registry(&LanguageRegistry::with_defaults());
        assert!(walker.includes(&root, &root.join("a.php")));
        assert!(walker.includes(&root, &root.join("src/deep/new.ts")));
        assert!(!walker.includes(&root, &root.join("notes.txt")));
        assert!(!walker.includes(&root, &root.join("vendor/x/a.php")));
        // a .gitignore mappái a gyökér alatti szinteken (mint a bejárásnál)
        assert!(!walker.includes(&root, &root.join("sub/build/a.php")));
        assert!(!walker.includes(&root, &root.join("crate/target/out.rs")));
        assert!(walker.includes(&root, &root.join("web/target/page.php")));
        assert!(!walker.includes(&root, Path::new("/elsewhere/a.php")));
        assert!(walker.includes_dir(&root, &root));
        assert!(walker.includes_dir(&root, &root.join("src")));
        assert!(!walker.includes_dir(&root, &root.join("node_modules/pkg")));
    }

    /// Az MCP-figyelő, a hook és a `refresh` szűrője (`includes`): kis-/nagybetűs kiterjesztés, generált fájl és a
    /// `.callgraphignore` ugyanúgy, mint a bejárásnál. A fájlnak nem kell léteznie.
    #[test]
    fn includes_applies_case_generated_and_ignore_file_rules() {
        let temp = TempDir::new();
        let root = temp.0.canonicalize().unwrap();
        let walker = FileWalker::for_registry(&LanguageRegistry::with_defaults());
        for name in [
            "src/A.php",
            "src/B.PHP",
            "src/App.Vue",
            "lib/x.TS",
            "README.md",
        ] {
            assert!(walker.includes(&root, &root.join(name)), "{name}");
        }
        for name in [
            "src/A.php.bak",
            "src/noext",
            "notes.txt",
            "package-lock.json",
            "dist/app.min.js",
            "dist/site.MIN.CSS",
            "vendor/x/A.php",
        ] {
            assert!(!walker.includes(&root, &root.join(name)), "{name}");
        }
        fs::write(root.join(IGNORE_FILE), "md\n*.Vue\n").unwrap();
        assert!(!walker.includes(&root, &root.join("README.md")));
        assert!(!walker.includes(&root, &root.join("src/App.vue")));
        assert!(walker.includes(&root, &root.join("src/A.php")));
        assert!(walker.excluded_by_ignore_file(&root.join("docs/guide.MD")));
        assert!(!walker.excluded_by_ignore_file(&root.join("src/A.php")));
    }

    #[test]
    fn ignore_file_lines_accept_bare_dotted_and_glob_extensions() {
        assert_eq!(
            parse_ignore("# comment\n\nmd\n.JSON\n*.vue\n  ts  \nsrc/\n*\n.\n*.d.ts\n"),
            ["md", "json", "vue", "ts"]
        );
        // minden más sor útminta (a `.` és a `*.` önmagában semmi)
        assert_eq!(
            path_patterns("# comment\nmd\n.JSON\nsrc/\n*\n.\n*.d.ts\n/public/build\n!keep.js\n"),
            ["src/", "*", "*.d.ts", "/public/build", "!keep.js"]
        );
    }

    #[test]
    fn nearest_ignore_file_wins() {
        let temp = TempDir::new();
        let root = temp.0.canonicalize().unwrap();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join(IGNORE_FILE), "md\n").unwrap();
        fs::write(root.join("a").join(IGNORE_FILE), "js\n").unwrap();
        assert_eq!(excluded_extensions(&root), ["md"]);
        assert_eq!(excluded_extensions(&root.join("a/b")), ["js"]);
    }
}
