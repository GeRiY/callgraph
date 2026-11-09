//! A bejárás kizárási szabályai (a `.gitignore` és a `.callgraphignore`), git-szemantikával: a `.gitignore`
//! mintái a saját mappájához relatívak (`/public/build` csak a mappa alatt közvetlenül, `public/build` bármely
//! szinten a mappa alatt, a `/`-re végződő csak mappára, `**`, `!`-es visszavétel, `#` megjegyzés), minden almappa
//! `.gitignore`-ja a saját mappájára hat, a mélyebb fájl mintája erősebb. A felmenők `.gitignore`-ja is számít, a
//! git-tároló gyökeréig (a `.git`-et tartalmazó mappa); tároló nélkül a fájlrendszer gyökeréig.
//!
//! A `.callgraphignore` soronként: `ext`, `.ext` vagy `*.ext` (csak betű / számjegy) → kiterjesztés-szabály, a
//! legközelebbi `.callgraphignore` érvényes (kis-/nagybetűtől függetlenül, mint eddig); minden más sor
//! `.gitignore`-minta a fájl mappájához relatívan (`public/build`, `/storage/`, `*.d.ts`, `!keep.js`). A
//! `.callgraphignore` mintái a `.gitignore` után jönnek, így felülírhatják (`!`).
//!
//! Kapcsoló: `--no-gitignore` (CLI) / `CALLGRAPH_GITIGNORE=0` (MCP, hook) — ekkor a `.gitignore` fájlok nem
//! számítanak; a `.callgraphignore` és a beépített kihagyott mappák igen. A projekt-gráf (`.callgraph/`) a két módot
//! nem keveri: kulcsa a fájl útja, és minden teljes hatókörű lekérdezés / `refresh` a bejárás aktuális fájllistájára
//! szűkíti (`ProjectGraph::refresh` `full`), így módváltás után a csak a másik módban látott fájlok kikerülnek (a
//! visszaváltás ezeket egyszer újraelemzi). Az MCP-szerver egy folyamaton belül egy módban fut (a változó induláskor).
//!
//! A minifikált / bundlolt JS / CSS biztonsági hálója nem itt van, hanem az elemzőben
//! (`lang::filetype::looks_minified`): ott a tartalom úgyis be van olvasva, a bejárásnak így nem kell minden JS / CSS
//! fájlba belenéznie (a meleg lekérdezés csak `stat`-ol), és a projekt-gráf megjegyzi az eredményt.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// A projektenkénti kizárások fájlja (a `.gitignore`-hoz hasonlóan a projektben, commitolható).
pub const IGNORE_FILE: &str = ".callgraphignore";
/// A git kizárási fájlja.
const GITIGNORE_FILE: &str = ".gitignore";
/// A `.gitignore` kikapcsolása: `CALLGRAPH_GITIGNORE=0` (a `--no-gitignore` kapcsoló is ezt állítja be, így a
/// gyermek-folyamatok — háttér-`refresh` — is öröklik).
pub const GITIGNORE_ENV: &str = "CALLGRAPH_GITIGNORE";

/// Figyelembe veszi-e a bejárás a `.gitignore` fájlokat (alapból igen; `CALLGRAPH_GITIGNORE=0` / `false` / `off`
/// / `no` kikapcsolja).
pub fn gitignore_enabled() -> bool {
    !std::env::var(GITIGNORE_ENV).is_ok_and(|value| switched_off(&value))
}

/// Kikapcsoló érték-e (`0`, `false`, `off`, `no`; kis-/nagybetűtől és szélső szóközöktől függetlenül).
fn switched_off(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no"
    )
}

/// A `dir`-re érvényes (a mappától felfelé a legközelebbi) `.callgraphignore` kizárt kiterjesztései, kisbetűvel.
pub fn excluded_extensions(dir: &Path) -> Vec<String> {
    let mut at = Some(dir);
    while let Some(candidate) = at {
        if let Ok(contents) = std::fs::read_to_string(candidate.join(IGNORE_FILE)) {
            return parse_ignore(&contents);
        }
        at = candidate.parent();
    }
    Vec::new()
}

/// A `.callgraphignore` kiterjesztés-szabályai (`md`, `.md`, `*.md` → `md`), kisbetűvel; a többi sor útminta
/// (`path_patterns`).
pub fn parse_ignore(contents: &str) -> Vec<String> {
    rule_lines(contents)
        .filter_map(extension_rule)
        .map(str::to_ascii_lowercase)
        .collect()
}

/// A `.callgraphignore` útmintái (minden sor, ami nem kiterjesztés-szabály), változatlanul.
pub fn path_patterns(contents: &str) -> Vec<&str> {
    rule_lines(contents)
        .filter(|line| extension_rule(line).is_none() && !matches!(*line, "." | "*."))
        .collect()
}

/// A nem üres, nem megjegyzés sorok, a szélső szóközök nélkül.
fn rule_lines(contents: &str) -> impl Iterator<Item = &str> {
    contents
        .lines()
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

/// `ext`, `.ext` vagy `*.ext` (az `ext` csak ASCII betű / számjegy) → az `ext`; különben `None`.
fn extension_rule(line: &str) -> Option<&str> {
    let ext = line
        .strip_prefix("*.")
        .or_else(|| line.strip_prefix('.'))
        .unwrap_or(line);
    (!ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric())).then_some(ext)
}

/// Egy mappára érvényes matcherek, a legkülsőtől a mappa sajátjáig.
type Layers = Rc<Vec<Rc<Gitignore>>>;

/// A mappánkénti szabályok egy bejáráshoz (vagy egy `includes` híváshoz): mappánként egyszer olvasva, a szülő
/// szabályaira építve.
pub(crate) struct IgnoreTree {
    gitignore: bool,
    layers: HashMap<PathBuf, Layers>,
}

impl IgnoreTree {
    /// `gitignore`: a `.gitignore` fájlok is számítanak-e (a `.callgraphignore` mindig).
    pub(crate) fn new(gitignore: bool) -> Self {
        Self {
            gitignore,
            layers: HashMap::new(),
        }
    }

    /// Kizárja-e a `dir` mappa egy bejegyzését (`path`: a `dir` alatti teljes út, ugyanabban az alakban) a mappára
    /// érvényes minták közül a legerősebb: a mélyebb fájlé, azon belül a későbbi sor.
    pub(crate) fn ignored(&mut self, dir: &Path, path: &Path, is_dir: bool) -> bool {
        for matcher in self.layers_for(dir).iter().rev() {
            let found = matcher.matched(path, is_dir);
            if found.is_ignore() {
                return true;
            }
            if found.is_whitelist() {
                return false;
            }
        }
        false
    }

    fn layers_for(&mut self, dir: &Path) -> Layers {
        if let Some(layers) = self.layers.get(dir) {
            return layers.clone();
        }
        // a git-tároló gyökere fölé nem megy (a tárolón kívüli `.gitignore` nem a projekté)
        let top = dir.join(".git").exists();
        let parent = match dir.parent() {
            Some(parent) if !top => self.layers_for(parent),
            _ => Rc::new(Vec::new()),
        };
        let layers = match self.own(dir) {
            Some(own) => {
                let mut all = parent.to_vec();
                all.push(Rc::new(own));
                Rc::new(all)
            }
            None => parent,
        };
        self.layers.insert(dir.to_path_buf(), layers.clone());
        layers
    }

    /// A mappa saját mintái: a `.gitignore` (ha be van kapcsolva), utána a `.callgraphignore` útmintái. Hibás
    /// minta kimarad (a git is figyelmen kívül hagyja).
    fn own(&self, dir: &Path) -> Option<Gitignore> {
        let mut builder = GitignoreBuilder::new(dir);
        let mut lines = 0usize;
        if self.gitignore {
            let file = dir.join(GITIGNORE_FILE);
            if let Ok(contents) = std::fs::read_to_string(&file) {
                for line in contents.lines() {
                    let line = line.trim_start_matches('\u{feff}');
                    if builder.add_line(Some(file.clone()), line).is_ok() {
                        lines += 1;
                    }
                }
            }
        }
        let file = dir.join(IGNORE_FILE);
        if let Ok(contents) = std::fs::read_to_string(&file) {
            for line in path_patterns(&contents) {
                if builder.add_line(Some(file.clone()), line).is_ok() {
                    lines += 1;
                }
            }
        }
        if lines == 0 {
            return None;
        }
        builder.build().ok().filter(|matcher| !matcher.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use crate::io::FileWalker;
    use crate::lang::LanguageRegistry;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Ideiglenes projekt: létrehozza a fájlokat, a végén törlődik.
    struct Project(PathBuf);

    impl Project {
        fn new(files: &[(&str, &str)]) -> Self {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "callgraph-ignore-{}-{}",
                std::process::id(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            let root = path.canonicalize().unwrap();
            for (name, text) in files {
                let file = root.join(name);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, text).unwrap();
            }
            Self(root)
        }

        fn walker(gitignore: bool) -> FileWalker {
            FileWalker::for_registry(&LanguageRegistry::with_defaults())
                .with_gitignore(gitignore)
        }

        /// A bejárt fájlok a gyökérhez relatívan, `/`-rel, rendezve.
        fn walked(&self, gitignore: bool) -> Vec<String> {
            let mut files: Vec<String> = Self::walker(gitignore)
                .collect(std::slice::from_ref(&self.0), &mut |_| {})
                .iter()
                .map(|file| rel(&self.0, file))
                .collect();
            files.sort();
            files
        }

        /// Az `includes` döntése a fájlokra (a bejárással egyeznie kell), rendezve.
        fn included(&self, names: &[&str], gitignore: bool) -> Vec<String> {
            let walker = Self::walker(gitignore);
            let mut files: Vec<String> = names
                .iter()
                .filter(|name| walker.includes(&self.0, &self.0.join(name)))
                .map(|name| name.to_string())
                .collect();
            files.sort();
            files
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn rel(root: &Path, file: &Path) -> String {
        file.strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    }

    const FILES: [&str; 6] = [
        "app/A.php",
        "public/build/assets/app-CBW7lKWe.js",
        "public/build/manifest.js",
        "public/index.php",
        "resources/build/keep.js",
        "resources/js/Comp.vue",
    ];
    const ASSET: &str = "public/build/assets/app-CBW7lKWe.js";
    const MANIFEST: &str = "public/build/manifest.js";
    const KEEP: &str = "resources/build/keep.js";

    /// A Laravel + Vite projekt a megadott ignore-fájllal.
    fn laravel(ignore_name: &str, ignore: &str) -> Project {
        let mut files: Vec<(&str, &str)> = FILES.iter().map(|name| (*name, "x")).collect();
        files.push((ignore_name, ignore));
        Project::new(&files)
    }

    /// A bejárás és az `includes` ugyanazt adja: a várt listát.
    fn assert_walks(project: &Project, gitignore: bool, expected: &[String], label: &str) {
        let walked = project.walked(gitignore);
        let included = project.included(&FILES, gitignore);
        assert_eq!(walked, expected, "{label} (gitignore: {gitignore})");
        assert_eq!(included, expected, "{label} (gitignore: {gitignore})");
    }

    /// A `FILES` a kihagyottak nélkül, rendezve.
    fn without(skip: &[&str]) -> Vec<String> {
        FILES
            .iter()
            .filter(|name| !skip.contains(name))
            .map(|name| name.to_string())
            .collect()
    }

    /// Minden mintaforma, a bejárásban és az `includes`-ban ugyanúgy; kikapcsolt `.gitignore`-ral nem hat.
    #[test]
    fn gitignore_pattern_forms() {
        let cases: [(&str, Vec<&str>); 10] = [
            // gyökérhez kötött
            ("/public/build\n", vec![ASSET, MANIFEST]),
            // a középen álló `/` is a mappához köt
            ("public/build\n", vec![ASSET, MANIFEST]),
            ("public/build/\n", vec![ASSET, MANIFEST]),
            ("public/build/**\n", vec![ASSET, MANIFEST]),
            // `/` nélkül: bármely szinten
            ("build/\n", vec![ASSET, MANIFEST, KEEP]),
            ("**/assets\n", vec![ASSET]),
            ("*.js\n", vec![ASSET, MANIFEST, KEEP]),
            // megjegyzés és visszavétel
            ("# build output\n*.js\n!manifest.js\n", vec![ASSET, KEEP]),
            // a gyökérhez kötött minta mélyebb szinten nem illeszkedik
            ("/build\n", vec![]),
            // csak mappára: a fájlra nem
            ("manifest.js/\n", vec![]),
        ];
        for (pattern, skip) in cases {
            let project = laravel(".gitignore", pattern);
            assert_walks(&project, true, &without(&skip), pattern);
            assert_walks(&project, false, &without(&[]), pattern);
        }
    }

    /// Almappa `.gitignore`-ja a saját mappájához relatív; a mélyebb minta erősebb; egy visszavett mappa
    /// `*.js` fájljai kizártak maradnak (mint a gitnél).
    #[test]
    fn nested_gitignore_is_relative_to_its_folder() {
        let project = Project::new(&[
            (".gitignore", "*.js\n!/build/\n"),
            ("public/.gitignore", "/build\n"),
            ("resources/.gitignore", "!keep.js\n"),
            ("public/build/app.php", "x"),
            ("build/tool.js", "x"),
            ("resources/keep.js", "x"),
            ("resources/drop.js", "x"),
            ("a.php", "x"),
        ]);
        assert_eq!(project.walked(true), ["a.php", "resources/keep.js"]);
        let names = [
            "public/build/app.php",
            "build/tool.js",
            "resources/keep.js",
            "resources/drop.js",
            "a.php",
        ];
        let included = project.included(&names, true);
        assert_eq!(included, ["a.php", "resources/keep.js"]);
        assert_eq!(project.included(&names, false).len(), names.len());
    }

    /// A git-tároló gyökere fölötti `.gitignore` nem számít; a tárolón belül a felmenőké igen (almappát bejárva is).
    #[test]
    fn ancestor_gitignores_up_to_the_repository_root() {
        let project = Project::new(&[
            (".gitignore", "*.php\n"),
            ("repo/.git/HEAD", "ref: x"),
            ("repo/.gitignore", "/app/gen/\n"),
            ("repo/app/A.php", "x"),
            ("repo/app/gen/G.php", "x"),
        ]);
        let files: Vec<String> = Project::walker(true)
            .collect(&[project.0.join("repo/app")], &mut |_| {})
            .iter()
            .map(|file| rel(&project.0, file))
            .collect();
        assert_eq!(files, ["repo/app/A.php"]);
    }

    /// `.callgraphignore`: az útminta `.gitignore` nélkül és kikapcsolt `.gitignore`-ral is hat; a kiterjesztés-sor
    /// a régi módon; a `.gitignore` után jön, így `!`-tel visszaveheti.
    #[test]
    fn callgraphignore_paths_and_extensions() {
        for pattern in ["public/build\n", "/public/build/\n", "public/build/**\n"] {
            let project = laravel(super::IGNORE_FILE, pattern);
            for gitignore in [true, false] {
                assert_walks(&project, gitignore, &without(&[ASSET, MANIFEST]), pattern);
            }
        }
        let project = laravel(super::IGNORE_FILE, "vue\n*.PHP\n");
        assert_eq!(
            project.walked(true),
            without(&["app/A.php", "public/index.php", "resources/js/Comp.vue"])
        );
        let project = Project::new(&[
            (".gitignore", "/public/build\n"),
            (super::IGNORE_FILE, "!/public/build\n"),
            ("public/build/app.js", "x"),
        ]);
        assert_eq!(project.walked(true), ["public/build/app.js"]);
    }

    #[test]
    fn switch_values() {
        for off in ["0", " OFF ", "false", "no"] {
            assert!(super::switched_off(off), "{off:?}");
        }
        for on in ["1", "", "yes", "true"] {
            assert!(!super::switched_off(on), "{on:?}");
        }
    }
}
