//! TS fogadó-típusok a `--grep` / `--depth` / `callers` kimenetben: a típusos property / paraméter / `new`
//! értékadás `Típus::metódus`-ra oldódik, és az interfészen át hívott metódus `via <Interfész>` jelölést kap
//! (a PHP-s `grep_output.rs` TS-megfelelője).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

impl Project {
    /// Egy kis TS-projekt: interfész, implementáció (típusos property + `new` értékadás), és egy controller,
    /// amely az interfészen, a konkrét osztályon, típusos paraméteren, `new`-val létrehozott változón és
    /// `any`-n át hívja a `getPlayables`-t.
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-grep-ts-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        let files = [
            ("package.json", "{}\n"),
            (
                "src/Resolver.ts",
                "export interface Resolver {\n  getPlayables(id: number): number[];\n}\n",
            ),
            (
                "src/Repo.ts",
                "export class Repo {\n  find(id: number): number[] {\n    return [id];\n  }\n}\n",
            ),
            (
                "src/PlaylistResolver.ts",
                concat!(
                    "import { Resolver } from './Resolver';\n",
                    "import { Repo } from './Repo';\n",
                    "\n",
                    "export class PlaylistResolver implements Resolver {\n",
                    "  private repo: Repo;\n",
                    "\n",
                    "  constructor() {\n",
                    "    this.repo = new Repo();\n",
                    "  }\n",
                    "\n",
                    "  getPlayables(id: number): number[] {\n",
                    "    return this.repo.find(id);\n",
                    "  }\n",
                    "}\n",
                ),
            ),
            (
                "src/Controller.ts",
                concat!(
                    "import type { Resolver } from './Resolver';\n",
                    "import { PlaylistResolver } from './PlaylistResolver';\n",
                    "\n",
                    "export class Controller {\n",
                    "  constructor(private resolver: Resolver, private playlists: PlaylistResolver) {}\n",
                    "\n",
                    "  index(id: number): number[] {\n",
                    "    return this.resolver.getPlayables(id);\n",
                    "  }\n",
                    "\n",
                    "  direct(id: number): number[] {\n",
                    "    const items = this.playlists.getPlayables(id);\n",
                    "    return items;\n",
                    "  }\n",
                    "}\n",
                    "\n",
                    "export function lookup(resolver: Resolver, id: number): number[] {\n",
                    "  return resolver.getPlayables(id);\n",
                    "}\n",
                    "\n",
                    "export function fresh(id: number): number[] {\n",
                    "  const r = new PlaylistResolver();\n",
                    "  return r.getPlayables(id);\n",
                    "}\n",
                    "\n",
                    "export function guess(x: any): number[] {\n",
                    "  return x.getPlayables(1);\n",
                    "}\n",
                ),
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
}

/// `src/a/B.ts` a platform elválasztójával (Windowson `src\a\B.ts`).
fn sep(path: &str) -> String {
    path.replace('/', std::path::MAIN_SEPARATOR_STR)
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn typed_property_call_matches_the_type_qualified_pattern() {
    let project = Project::new();
    let out = project.run(&["show", "src", "--grep", "Repo::find"]);
    // egyetlen fájl a kimenetben: az út abszolút marad (nincs `root:` sor)
    assert!(
        out.lines()
            .any(|l| l.starts_with("FILE  ") && l.ends_with(&sep("/src/PlaylistResolver.ts"))),
        "{out}"
    );
    // a fájlnévvel azonos nevű osztály metódusa egyszer (nem a `module PlaylistResolver` alatt is)
    assert_eq!(
        out.matches("  getPlayables()  L11-13\n").count(),
        1,
        "{out}"
    );
    assert!(
        out.contains("    L12 │ return this.repo.find(id);\n"),
        "{out}"
    );
}

#[test]
fn concrete_method_search_finds_ts_calls_through_the_interface() {
    let project = Project::new();
    let out = project.run(&["show", "src", "--grep", "PlaylistResolver::getPlayables"]);
    // konstruktor paraméter-property az interfész típusával
    assert!(out.contains("  index()  L7-9\n"), "{out}");
    assert!(
        out.contains("    L8 via Resolver │ return this.resolver.getPlayables(id);\n"),
        "{out}"
    );
    // a konkrét osztály típusával: nincs `via`
    assert!(
        out.contains("    L12 │ const items = this.playlists.getPlayables(id);\n"),
        "{out}"
    );
    // típusos paraméter (interfész) és `new`-val létrehozott változó
    assert!(
        out.contains("    L18 via Resolver │ return resolver.getPlayables(id);\n"),
        "{out}"
    );
    assert!(
        out.contains("    L23 │ return r.getPlayables(id);\n"),
        "{out}"
    );
    // `any`: a fogadó típusa ismeretlen, nem találat
    assert!(!out.contains("guess()"), "{out}");
}

#[test]
fn depth_follows_the_ts_interface_upwards() {
    let project = Project::new();
    let out = project.run(&["show", "src", "--grep", "Repo::find", "--depth", "2"]);
    assert!(
        out.contains(&format!(
            "  {}  PlaylistResolver::getPlayables()  L11-13\n    L12 │ return this.repo.find(id);\n",
            sep("src/PlaylistResolver.ts")
        )),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "  {}  Controller::index()  L7-9  ← Resolver::getPlayables\n    L8 via Resolver │ return this.resolver.getPlayables(id);\n",
            sep("src/Controller.ts")
        )),
        "{out}"
    );
}

#[test]
fn callers_jsonl_marks_ts_interface_matches() {
    let project = Project::new();
    let out = project.run(&["callers", "PlaylistResolver::getPlayables"]);
    let rows: Vec<serde_json::Value> = out
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let row = |caller: &str| {
        rows.iter()
            .find(|row| row["caller"] == caller)
            .unwrap_or_else(|| panic!("{caller}: {out}"))
    };
    assert_eq!(row("index")["via"], "Resolver");
    assert_eq!(row("lookup")["via"], "Resolver");
    assert!(row("direct").get("via").is_none(), "{out}");
    assert!(row("fresh").get("via").is_none(), "{out}");
}

#[test]
fn class_named_like_its_file_lists_each_member_once() {
    let project = Project::new();
    fs::write(
        project.0.join("src/Playlist.ts"),
        concat!(
            "export class Playlist {\n",
            "  play(): void {\n",
            "    helper();\n",
            "  }\n",
            "}\n",
            "\n",
            "export function helper(): void {}\n",
        ),
    )
    .unwrap();
    let out = project.run(&["show", "src/Playlist.ts"]);
    assert_eq!(out.matches("  play()  L2-4").count(), 1, "{out}");
    assert_eq!(out.matches("  helper()  L7-7").count(), 1, "{out}");
    // a top-level függvény a modulé, az osztálymetódus az osztályé
    let at = |needle: &str| {
        out.find(needle)
            .unwrap_or_else(|| panic!("{needle}: {out}"))
    };
    assert!(at("module Playlist") < at("  helper()"), "{out}");
    assert!(at("  helper()") < at("class Playlist"), "{out}");
    assert!(at("class Playlist") < at("  play()"), "{out}");
    // `--grep`: a hívó egyszer, csak az osztály alatt
    let out = project.run(&["show", "src", "--grep", "^helper$"]);
    assert_eq!(out.matches("  play()  L2-4").count(), 1, "{out}");
    assert!(!out.contains("module Playlist"), "{out}");
    let out = project.run(&["callers", "helper"]);
    assert_eq!(
        out.lines().filter(|l| l.contains("\"play\"")).count(),
        1,
        "{out}"
    );
}
