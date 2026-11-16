//! CSS / SCSS a parancssorból (valódi binárissal): a hívások (`var`, `@include`, függvények) a szabályuk alatt
//! jelennek meg az outline-ban, és a `--grep` megtalálja őket; a JSON nagy fájljának tartalék nézete és a summary
//! számlálói (kulcs / szabály) is itt vannak.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const APP_CSS: &str = ":root {
  --brand: red;
}

.card {
  color: var(--brand);
}

@media (min-width: 640px) {
  .card .title {
    margin: calc(1px + 2px);
  }
}
";

const APP_SCSS: &str = "@mixin pad($n) {
  padding: map-get($sizes, $n);
}

.card {
  @include pad(2);
}
";

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "callgraph-style-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("res")).unwrap();
        let project = Self(root.canonicalize().unwrap());
        fs::write(project.0.join("res/app.css"), APP_CSS).unwrap();
        fs::write(project.0.join("res/app.scss"), APP_SCSS).unwrap();
        let rows: Vec<String> = (0..450)
            .map(|i| format!("  \"key_{i}\": \"value {i}\""))
            .collect();
        fs::write(
            project.0.join("res/en.json"),
            format!("{{\n{}\n}}\n", rows.join(",\n")),
        )
        .unwrap();
        project
    }

    fn run(&self, args: &[&str], graph: bool) -> String {
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
        let out = command.output().expect("callgraph runs");
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
fn css_outline_lists_var_and_function_calls_under_their_rule() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["show", "res/app.css"], graph);
        // a keresés a szabály sorától indul: a FILE-fejléc útvonala (macOS: /private/var/…) is tartalmazhat „var”-t
        let card = out.find("  .card  L5-7").expect("card rule");
        assert!(out[card..].contains("var"), "{graph}: {out}");
        let title = out.find("  .card .title").expect("title rule");
        assert!(out[title..].contains("calc"), "{graph}: {out}");
    }
}

#[test]
fn css_grep_finds_the_rule_that_calls_var() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["show", "res", "--grep", "var"], graph);
        assert!(out.contains(".card"), "{graph}: {out}");
        assert!(out.contains("app"), "{graph}: {out}");
    }
}

#[test]
fn scss_outline_and_grep_show_includes_and_functions_under_their_rule() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["show", "res/app.scss"], graph);
        let card = out.find("  .card  L5-7").unwrap_or(usize::MAX);
        assert!(card < out.rfind("include").unwrap_or(0), "{graph}: {out}");
        let mixin = out.find("  pad()  L1-3").unwrap_or(usize::MAX);
        assert!(mixin < out.find("map-get").unwrap_or(0), "{graph}: {out}");
        let grep = project.run(&["show", "res", "--grep", "map-get"], graph);
        assert!(grep.contains("pad"), "{graph}: {grep}");
    }
}

#[test]
fn big_json_outline_fallback_is_a_short_key_index() {
    let project = Project::new();
    let out = project.run(&["show", "res/en.json", "--format", "method-index"], true);
    assert!(out.contains("key_0 … key_449  450 string  L2-451"), "{out}");
    assert!(out.lines().count() <= 25, "{out}");
    let summary = project.run(&["show", "res/en.json", "--format", "summary"], true);
    assert!(summary.contains("(450 keys)"), "{summary}");
}
