//! Csak a támogatott fájlok feldolgozása: regisztrált kiterjesztés (kis-/nagybetűtől függetlenül), generált fájl
//! (`package-lock.json`, `*.min.js`, `*.min.css`) és bináris tartalom nélkül, minden fájl egyszer (symlink), és a
//! projekt `.callgraphignore` fájljában kizárt kiterjesztések nélkül. A mappa-bejárás, a közvetlenül megadott fájl,
//! a string-keresés, a `refresh`, az `analyze` és a projekt-gráf nélküli út ugyanígy viselkedik.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const PHP: &str = "<?php\nclass A\n{\n    public function run()\n    {\n        return $this->go('needle-php');\n    }\n\n    public function go($x)\n    {\n        return $x;\n    }\n}\n";

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-supported-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        fs::create_dir_all(&root).unwrap();
        let project = Self(root.canonicalize().unwrap());
        // támogatott
        project.write("src/A.php", PHP);
        project.write("src/Upper.PHP", &PHP.replace("class A", "class Upper"));
        project.write(
            "src/app.js",
            "export function f() {\n  return g('needle-js');\n}\nfunction g(x) { return x; }\n",
        );
        project.write(
            "src/Typed.TS",
            "interface Shape { area(): number }\nexport function size(s: Shape): number {\n  return s.area();\n}\n",
        );
        project.write("docs/guide.md", "# Guide\n\nneedle-md\n");
        project.write(
            "package.json",
            "{\"name\": \"demo\", \"version\": \"1.0.0\"}\n",
        );
        // nem támogatott kiterjesztés (a tartalmuk a keresett szöveget is tartalmazza)
        for name in [
            "src/A.php.bak",
            "src/A.php.orig",
            "src/noext",
            "src/script.py",
            "docs/notes.txt",
            "docs/conf.yml",
            "docs/data.csv",
            "composer.lock",
        ] {
            project.write(name, "needle-unsupported <?php function f() {}\n");
        }
        // generált fájlok támogatott kiterjesztéssel
        project.write(
            "package-lock.json",
            "{\"name\": \"demo\", \"packages\": {\"needle-generated\": {}}}\n",
        );
        project.write(
            "dist/app.min.js",
            "function f(){return g('needle-generated')}\n",
        );
        project.write(
            "dist/APP.MIN.JS",
            "function h(){return g('needle-generated')}\n",
        );
        project.write("dist/site.min.css", ".needle-generated{color:red}\n");
        // bináris tartalom támogatott kiterjesztéssel
        project.write_bytes("assets/blob.js", b"needle-binary\0\x01\x02\xff\xfe binary");
        project.write_bytes(
            "assets/image.php",
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR needle-binary",
        );
        project
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    fn write(&self, rel: &str, text: &str) {
        self.write_bytes(rel, text.as_bytes());
    }

    fn write_bytes(&self, rel: &str, bytes: &[u8]) {
        let path = self.path(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn command(&self, args: &[&str], graph: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env_remove("CALLGRAPH_NO_GRAPH");
        if !graph {
            command.env("CALLGRAPH_NO_GRAPH", "1");
        }
        command.output().expect("callgraph runs")
    }

    fn run(&self, args: &[&str], graph: bool) -> String {
        let out = self.command(args, graph);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// A hibás futás stderr-je (a futásnak sikertelennek kell lennie).
    fn fail(&self, args: &[&str], graph: bool) -> String {
        let out = self.command(args, graph);
        assert!(
            !out.status.success(),
            "{args:?} should fail: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        String::from_utf8(out.stderr).unwrap()
    }

    /// A mappa bejárásakor feldolgozott fájlok (a `FILE` sorokból), a gyökérhez relatívan, `/`-rel, rendezve.
    fn listed(&self, dir: &str, graph: bool) -> Vec<String> {
        let out = self.run(&["show", dir, "--full"], graph);
        let root = format!("{}{}", self.0.display(), std::path::MAIN_SEPARATOR);
        let mut files: Vec<String> = out
            .lines()
            .filter_map(|line| line.strip_prefix("FILE  "))
            .map(|rest| rest.split("  ").next().unwrap_or(rest))
            .map(|path| path.trim_start_matches(&root).replace('\\', "/"))
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

const SUPPORTED: [&str; 6] = [
    "docs/guide.md",
    "package.json",
    "src/A.php",
    "src/Typed.TS",
    "src/Upper.PHP",
    "src/app.js",
];

#[test]
fn directory_scan_lists_only_supported_files_with_and_without_the_project_graph() {
    let project = Project::new();
    for graph in [true, false] {
        assert_eq!(project.listed(".", graph), SUPPORTED, "graph: {graph}");
    }
    // a gráf második (meleg) futása is ugyanezt adja
    assert_eq!(project.listed(".", true), SUPPORTED);
}

#[test]
fn uppercase_extensions_are_parsed_with_their_language() {
    let project = Project::new();
    let php = project.run(&["show", "src/Upper.PHP"], true);
    assert!(php.contains("class Upper"), "{php}");
    assert!(php.contains("run()") && php.contains("go("), "{php}");
    // a .TS TypeScriptként parsolódik (az interface nem szintaktikai hiba)
    let ts = project.run(&["show", "src/Typed.TS"], true);
    assert!(ts.contains("size("), "{ts}");
}

#[test]
fn explicit_unsupported_generated_and_binary_files_are_refused_with_a_reason() {
    let project = Project::new();
    for graph in [true, false] {
        for name in [
            "docs/notes.txt",
            "src/A.php.bak",
            "src/noext",
            "composer.lock",
        ] {
            let err = project.fail(&["show", name], graph);
            assert!(err.contains("no language registered"), "{name}: {err}");
        }
        for name in [
            "package-lock.json",
            "dist/app.min.js",
            "dist/APP.MIN.JS",
            "dist/site.min.css",
        ] {
            let err = project.fail(&["show", name], graph);
            assert!(err.contains("generated file"), "{name}: {err}");
        }
        for name in ["assets/blob.js", "assets/image.php"] {
            let err = project.fail(&["show", name], graph);
            assert!(err.contains("binary file"), "{name}: {err}");
        }
    }
}

#[test]
fn string_search_never_reads_skipped_files() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["show", ".", "--strings", "needle"], graph);
        assert!(out.contains("needle-php"), "{out}");
        assert!(out.contains("needle-js"), "{out}");
        for skipped in ["needle-unsupported", "needle-generated", "needle-binary"] {
            assert!(!out.contains(skipped), "{skipped}: {out}");
        }
    }
}

#[test]
fn analyze_jsonl_contains_only_supported_files() {
    let project = Project::new();
    let out = project.command(&["analyze", "."], true);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    let mut files: Vec<String> = stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .map(|row| {
            let file = row["file"].as_str().unwrap().to_owned();
            file.trim_start_matches(&format!(
                "{}{}",
                project.0.display(),
                std::path::MAIN_SEPARATOR
            ))
            .replace('\\', "/")
        })
        .collect();
    files.sort();
    assert_eq!(files, SUPPORTED, "{stderr}");
    // a bináris fájlokról egy megjegyzés, nem sor
    assert!(stderr.contains("blob.js: binary file"), "{stderr}");
}

#[test]
fn refresh_tracks_only_supported_files() {
    let project = Project::new();
    let out = project.run(&["refresh"], true);
    // 6 támogatott + 2 bináris (a gráf megjegyzi őket, hogy ne olvassa újra; a lekérdezések kihagyják)
    assert!(out.contains("8 updated"), "{out}");
    let again = project.run(&["refresh"], true);
    assert!(
        again.contains("0 updated") && again.contains("8 unchanged"),
        "{again}"
    );
    // nem támogatott, generált út megadva: semmi
    let named = project.run(
        &["refresh", "docs/notes.txt", "package-lock.json", "dist"],
        true,
    );
    assert!(
        named.contains("0 updated, 0 removed, 0 unchanged"),
        "{named}"
    );
    assert_eq!(project.listed(".", true), SUPPORTED);
}

#[test]
fn callgraphignore_excludes_extensions_per_project() {
    let project = Project::new();
    project.write(
        ".callgraphignore",
        "# a dokumentáció és a csomag-leírás ne kerüljön be\nmd\n\n.JSON\n",
    );
    let expected = ["src/A.php", "src/Typed.TS", "src/Upper.PHP", "src/app.js"];
    for graph in [true, false] {
        assert_eq!(project.listed(".", graph), expected, "graph: {graph}");
        let err = project.fail(&["show", "docs/guide.md"], graph);
        assert!(err.contains("excluded by .callgraphignore"), "{err}");
        let out = project.run(&["show", ".", "--strings", "needle"], graph);
        assert!(!out.contains("needle-md"), "{out}");
    }
    // a refresh is kihagyja, és a korábban felvett fájlok kikerülnek a gráfból
    let out = project.run(&["refresh"], true);
    assert!(
        out.contains("unchanged") || out.contains("updated"),
        "{out}"
    );
    assert_eq!(project.listed(".", true), expected);
}

#[test]
fn nearest_callgraphignore_applies_to_a_subfolder() {
    let project = Project::new();
    project.write(".callgraphignore", "*.md\n");
    // a src/ saját fájlja felülírja a gyökérét: ott a js kimarad, az md (a gyökér szabálya) itt nem érvényes
    project.write("src/.callgraphignore", "js\n");
    project.write("src/inner.md", "# Inner\n");
    assert_eq!(
        project.listed(".", true),
        [
            "package.json",
            "src/A.php",
            "src/Typed.TS",
            "src/Upper.PHP",
            "src/inner.md"
        ]
    );
}

#[test]
fn a_file_is_processed_once_even_through_a_symlink() {
    #[cfg(unix)]
    {
        let project = Project::new();
        fs::create_dir_all(project.path("out")).unwrap();
        std::os::unix::fs::symlink(project.path("src/A.php"), project.path("out/link.php"))
            .unwrap();
        std::os::unix::fs::symlink(project.path("src"), project.path("out/src-link")).unwrap();
        for graph in [true, false] {
            let out = project.run(&["show", ".", "--full"], graph);
            let a = out
                .lines()
                .filter(|line| line.starts_with("FILE") && line.contains("A.php"))
                .count();
            assert_eq!(a, 1, "{out}");
        }
    }
}

#[test]
fn unsupported_files_never_reach_the_project_graph() {
    let project = Project::new();
    project.run(&["show", "."], true);
    let pack = fs::read(project.path(".callgraph/graph.pack")).unwrap();
    let text = String::from_utf8_lossy(&pack);
    for name in [
        "notes.txt",
        "A.php.bak",
        "noext",
        "package-lock.json",
        "app.min.js",
        "site.min.css",
    ] {
        assert!(!text.contains(name), "{name} is in the graph");
    }
    assert!(Path::new(&project.path(".callgraph/index.bin")).is_file());
}
