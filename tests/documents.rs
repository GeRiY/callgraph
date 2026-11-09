//! Dokumentum- és adatfájlok a parancssorból: a JSON kulcs-útja `::` szelektorként a `--body`-val a kulcs pontos
//! JSON-részfáját adja (tömb-elem indexszel is), a `locate` megtalálja a JSON-kulcsot; a projekt-gráffal és nélküle.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const PACKAGE: &str = r#"{
    "private": true,
    "type": "module",
    "scripts": {
        "build": "vite build",
        "dev": "vite"
    },
    "devDependencies": {
        "laravel-vite-plugin": "^1.0",
        "vite": "^6.0"
    },
    "files": [
        "dist",
        "src"
    ]
}
"#;

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "callgraph-documents-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("resources/js")).unwrap();
        let project = Self(root.canonicalize().unwrap());
        fs::write(project.0.join("package.json"), PACKAGE).unwrap();
        fs::write(
            project.0.join("resources/js/app.js"),
            "export function boot() {\n  return 1;\n}\n",
        )
        .unwrap();
        project
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
            .env("CALLGRAPH_STATS", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
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
        String::from_utf8(out.stdout).unwrap().replace('\\', "/")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn json_key_path_selector_prints_the_key_subtree() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["show", "package.json::scripts", "--body"], graph);
        assert!(
            out.contains("package.json::scripts  L4-7\n"),
            "{graph}: {out}"
        );
        for line in [
            "     4      \"scripts\": {\n",
            "     5          \"build\": \"vite build\",\n",
            "     7      },\n",
        ] {
            assert!(out.contains(line), "{graph} {line:?}: {out}");
        }
        assert!(!out.contains("devDependencies"), "{graph}: {out}");

        let out = project.run(&["show", "package.json::scripts.build", "--body"], graph);
        assert!(
            out.contains("package.json::scripts.build  L5-5\n"),
            "{graph}: {out}"
        );
        assert!(!out.contains("\"dev\""), "{graph}: {out}");

        let out = project.run(&["show", "package.json::files.1", "--body"], graph);
        assert!(
            out.contains("package.json::files.1  L14-14\n"),
            "{graph}: {out}"
        );
        assert!(out.contains("\"src\""), "{graph}: {out}");

        // ismeretlen kulcsnál a régi hiba
        let out = project.command(&["show", "package.json::scripts.nope", "--body"], graph);
        assert!(!out.status.success(), "{graph}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("scripts.nope"),
            "{graph}"
        );
    }
}

#[test]
fn json_outline_shows_values_and_locate_finds_a_json_key() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["show", "package.json"], graph);
        assert!(
            out.contains("    scripts.build = \"vite build\"\n"),
            "{graph}: {out}"
        );
        assert!(out.contains("    files = array (2)\n"), "{graph}: {out}");

        let out = project.run(&["locate", "laravel-vite-plugin"], graph);
        assert!(out.contains("1. package.json"), "{graph}: {out}");
        assert!(
            out.contains("devDependencies.laravel-vite-plugin  L9"),
            "{graph}: {out}"
        );
    }
}
