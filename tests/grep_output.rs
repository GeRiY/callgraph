//! A `--grep` / `--depth` / `callers` kimenete: a gyökér egyszer, utána relatív utak; a hívás forrássora a hívó
//! mellett; és az interfészen át hívott metódus (`via <Interfész>`).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

impl Project {
    /// Egy kis PHP-projekt: interfész, implementáció, és egy controller, amely egyszer az interfészen,
    /// egyszer a konkrét osztályon át hívja a `getPlayables`-t.
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("callgraph-grep-{id}-{}-{}", std::process::id(), {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }));
        let files = [
            (
                "src/Resolver.php",
                "<?php\nnamespace App\\Contracts;\n\ninterface Resolver\n{\n    public function getPlayables(int $id): array;\n}\n",
            ),
            (
                "src/Repo.php",
                "<?php\nnamespace App\\Services;\n\nclass Repo\n{\n    public function find(int $id): array\n    {\n        return [];\n    }\n}\n",
            ),
            (
                "src/PlaylistResolver.php",
                "<?php\nnamespace App\\Services;\n\nuse App\\Contracts\\Resolver;\n\nclass PlaylistResolver implements Resolver\n{\n    private Repo $repo;\n\n    public function getPlayables(int $id): array\n    {\n        return $this->repo->find($id);\n    }\n}\n",
            ),
            (
                "src/Controller.php",
                "<?php\nnamespace App\\Http;\n\nuse App\\Contracts\\Resolver;\nuse App\\Services\\PlaylistResolver;\n\nclass Controller\n{\n    private Resolver $resolver;\n    private PlaylistResolver $playlists;\n\n    public function index(int $id): array\n    {\n        return $this->resolver->getPlayables($id);\n    }\n\n    public function direct(int $id): array\n    {\n        $items   =   $this->playlists->getPlayables(   $id   );\n        return $items;\n    }\n}\n",
            ),
        ];
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self(root.canonicalize().unwrap())
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .output()
            .expect("callgraph runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// A kiírt gyökér: Windowson a `\\?\` előtag nélkül (ahogy a callgraph is kiírja).
    fn root(&self) -> String {
        let root = self.0.to_string_lossy().into_owned();
        match root.strip_prefix(r"\\?\") {
            Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_owned(),
            _ => root,
        }
    }
}

/// `src/a/B.php` a platform elválasztójával (Windowson `src\a\B.php`).
fn sep(path: &str) -> String {
    path.replace('/', std::path::MAIN_SEPARATOR_STR)
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn grep_prints_root_once_and_relative_paths() {
    let project = Project::new();
    let out = project.run(&["show", "src", "--grep", "getPlayables|Repo::find"]);
    let root = project.root();
    assert_eq!(
        out.lines().next(),
        Some(format!("root: {root}").as_str()),
        "{out}"
    );
    assert_eq!(out.matches(root.as_str()).count(), 1, "{out}");
    assert!(
        out.contains(&format!(
            "FILE  {}  NS:App\\Http",
            sep("src/Controller.php")
        )),
        "{out}"
    );
    assert!(
        out.contains(&format!("FILE  {}", sep("src/PlaylistResolver.php"))),
        "{out}"
    );
}

#[test]
fn single_path_output_stays_absolute_without_root_line() {
    let project = Project::new();
    let out = project.run(&["show", "src/Controller.php"]);
    let root = project.root();
    assert!(!out.starts_with("root: "), "{out}");
    assert!(
        out.contains(&format!("FILE  {root}{}", sep("/src/Controller.php"))),
        "{out}"
    );
}

#[test]
fn grep_shows_the_trimmed_call_line_next_to_each_caller() {
    let project = Project::new();
    let out = project.run(&["show", "src", "--grep", "getPlayables"]);
    assert!(
        out.contains("    L19 │ $items = $this->playlists->getPlayables( $id );\n"),
        "{out}"
    );
    assert!(
        out.contains("    L14 │ return $this->resolver->getPlayables($id);\n"),
        "{out}"
    );
}

#[test]
fn concrete_method_search_finds_calls_through_the_interface() {
    let project = Project::new();
    let out = project.run(&["show", "src", "--grep", "PlaylistResolver::getPlayables"]);
    assert!(
        out.contains("  index()  L12-15\n"),
        "the interface-typed caller is missing:\n{out}"
    );
    assert!(
        out.contains("    L14 via Resolver │ return $this->resolver->getPlayables($id);\n"),
        "{out}"
    );
    assert!(out.contains("    L19 │ $items"), "{out}");
}

#[test]
fn depth_lists_call_lines_and_follows_the_interface_upwards() {
    let project = Project::new();
    let out = project.run(&["show", "src", "--grep", "Repo::find", "--depth", "2"]);
    let root = project.root();
    assert!(out.starts_with(&format!("root: {root}\n")), "{out}");
    assert!(
        out.contains(&format!("  {}  PlaylistResolver::getPlayables()  L10-13\n    L12 │ return $this->repo->find($id);\n", sep("src/PlaylistResolver.php"))),
        "{out}"
    );
    // 2. szint: a controller a Resolver interfészen át hívja a PlaylistResolver::getPlayables-t
    assert!(
        out.contains(&format!("  {}  Controller::index()  L12-15  ← App\\Contracts\\Resolver::getPlayables\n    L14 via Resolver │ return $this->resolver->getPlayables($id);\n", sep("src/Controller.php"))),
        "{out}"
    );
    assert!(
        out.contains(&format!("  {}  Controller::direct()  L17-21  ← App\\Services\\PlaylistResolver::getPlayables\n    L19 │ $items", sep("src/Controller.php"))),
        "{out}"
    );
}

#[test]
fn callers_jsonl_marks_interface_matches() {
    let project = Project::new();
    let out = project.run(&["callers", "PlaylistResolver::getPlayables"]);
    let rows: Vec<serde_json::Value> = out
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let index = rows
        .iter()
        .find(|row| row["caller"] == "index")
        .unwrap_or_else(|| panic!("{out}"));
    assert_eq!(index["via"], "Resolver");
    let direct = rows.iter().find(|row| row["caller"] == "direct").unwrap();
    assert!(direct.get("via").is_none(), "{out}");
}

#[test]
fn depth_searches_the_whole_project_from_level_two() {
    let project = Project::new();
    // csak a src/Services/ a lekérdezés; a 2. szintű hívó a projekt egy másik mappájában (src/Http/) van
    let files = [
        (
            "src/Services/Lookup.php",
            "<?php\nnamespace App\\Services;\n\nclass Lookup\n{\n    private Repo $repo;\n\n    public function run(int $id): array\n    {\n        return $this->repo->find($id);\n    }\n}\n",
        ),
        (
            "src/Http/Admin.php",
            "<?php\nnamespace App\\Http;\n\nuse App\\Services\\Lookup;\n\nclass Admin\n{\n    private Lookup $lookup;\n\n    public function show(int $id): array\n    {\n        return $this->lookup->run($id);\n    }\n}\n",
        ),
    ];
    for (path, text) in files {
        let path = project.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let out = project.run(&[
        "show",
        "src/Services",
        "--grep",
        "Repo::find",
        "--depth",
        "2",
    ]);
    assert!(
        out.contains(&format!(
            "  {}  Lookup::run()  L8-11\n",
            sep("src/Services/Lookup.php")
        )),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "  {}  Admin::show()  L10-13  ← App\\Services\\Lookup::run\n",
            sep("src/Http/Admin.php")
        )),
        "{out}"
    );
}

/// Egy hívó metódus: két azonos sorú hívás (összevonandó) és még négy különböző — a 3 soros korláthoz.
fn write_many(project: &Project) {
    fs::write(
        project.0.join("src/Many.php"),
        "<?php\nnamespace App\\Services;\n\nclass Many\n{\n    private Repo $repo;\n\n    public function run(int $id): array\n    {\n        $this->repo->find($id);\n        $this->repo->find(1);\n        $this->repo->find($id);\n        $this->repo->find(2);\n        $this->repo->find(3);\n        $this->repo->find(4);\n        return [];\n    }\n}\n",
    )
    .unwrap();
}

#[test]
fn grep_merges_identical_call_lines_and_caps_them_per_caller() {
    let project = Project::new();
    write_many(&project);
    let out = project.run(&["show", "src/Many.php", "--grep", "Repo::find"]);
    assert!(out.contains("  run()  L8-17\n"), "{out}");
    assert!(
        out.contains(concat!(
            "    L10, L12 │ $this->repo->find($id);\n",
            "    L11 │ $this->repo->find(1);\n",
            "    L13 │ $this->repo->find(2);\n",
            "    … +2 more: L14, L15\n",
        )),
        "{out}"
    );
    assert!(!out.contains("L14 │"), "{out}");
}

#[test]
fn grep_fallback_merges_and_caps_call_lines_too() {
    let project = Project::new();
    write_many(&project);
    let out = project.run(&[
        "show",
        "src/Many.php",
        "--grep",
        "Repo::find",
        "CALLGRAPH_OUT_LIMIT=6",
    ]);
    let file = sep("src/Many.php");
    let expected = format!(
        "{file}  Many::run  L10, L12  ← App\\Services\\Repo::find │ $this->repo->find($id);\n\
         {file}  Many::run  L11  ← App\\Services\\Repo::find │ $this->repo->find(1);\n\
         {file}  Many::run  L13  ← App\\Services\\Repo::find │ $this->repo->find(2);\n\
         {file}  Many::run  … +2 more: L14, L15\n"
    );
    assert!(out.contains("1 line per call instead"), "{out}");
    assert!(out.contains(&expected), "{out}");
}

#[test]
fn depth_level_one_merges_identical_call_lines() {
    let project = Project::new();
    write_many(&project);
    let out = project.run(&["show", "src", "--grep", "Repo::find", "--depth", "2"]);
    assert!(
        out.contains(&format!(
            "  {}  Many::run()  L8-17\n    L10, L12 │ $this->repo->find($id);\n    L11 │ $this->repo->find(1);\n    L13 │ $this->repo->find(2);\n    … +2 more: L14, L15\n",
            sep("src/Many.php")
        )),
        "{out}"
    );
}
