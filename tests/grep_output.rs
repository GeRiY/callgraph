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
        let root = std::env::temp_dir().join(format!("callgraph-grep-{id}"));
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

    fn root(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
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
        out.contains("FILE  src/Controller.php  NS:App\\Http"),
        "{out}"
    );
    assert!(out.contains("FILE  src/PlaylistResolver.php"), "{out}");
}

#[test]
fn single_path_output_stays_absolute_without_root_line() {
    let project = Project::new();
    let out = project.run(&["show", "src/Controller.php"]);
    let root = project.root();
    assert!(!out.starts_with("root: "), "{out}");
    assert!(
        out.contains(&format!("FILE  {root}/src/Controller.php")),
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
        out.contains(
            "  src/PlaylistResolver.php  PlaylistResolver::getPlayables()  L10-13\n    L12 │ return $this->repo->find($id);\n"
        ),
        "{out}"
    );
    // 2. szint: a controller a Resolver interfészen át hívja a PlaylistResolver::getPlayables-t
    assert!(
        out.contains("  src/Controller.php  Controller::index()  L12-15  ← App\\Contracts\\Resolver::getPlayables\n    L14 via Resolver │ return $this->resolver->getPlayables($id);\n"),
        "{out}"
    );
    assert!(
        out.contains("  src/Controller.php  Controller::direct()  L17-21  ← App\\Services\\PlaylistResolver::getPlayables\n    L19 │ $items"),
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
fn depth_searches_the_whole_query_scope_not_only_the_first_level_folders() {
    let project = Project::new();
    // 1. szint a src/Services/-ben, a 2. szintű hívó egy testvér-mappában (src/Http/)
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
        "src/Http",
        "--grep",
        "Repo::find",
        "--depth",
        "2",
    ]);
    assert!(
        out.contains("  src/Services/Lookup.php  Lookup::run()  L8-11\n"),
        "{out}"
    );
    assert!(
        out.contains("  src/Http/Admin.php  Admin::show()  L10-13  ← App\\Services\\Lookup::run\n"),
        "{out}"
    );
}
