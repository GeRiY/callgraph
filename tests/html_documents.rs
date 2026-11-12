//! HTML-dokumentumok a parancssorból (valódi binárissal, a projekt-gráffal és nélküle): a `<title>` és a
//! `<h1>`–`<h6>` címsorok outline-ja sortartománnyal, a `--lines`, a `--strings` a látható szövegben és a szöveges
//! attribútumokban (a `<script>`, `<style>` és a komment tartalma nem találat), a hibás / nem zárt HTML pánik nélkül.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const DESIGN: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <title>Checkout redesign</title>
  <style>.banner::after { content: "style-secret"; }</style>
</head>
<body>
  <!-- comment-secret -->
  <h1>Overview</h1>
  <p>The payment flow needs a refund button.</p>
  <h2>Goals</h2>
  <input placeholder="Search refunds" aria-label="Refund search">
  <img src="flow.png" alt="Refund flow diagram">
  <h2>Risks</h2>
  <p>Legacy
    gateway timeouts</p>
  <script>const label = "script-secret"; document.title = "<h1>Fake</h1>";</script>
</body>
</html>
"#;

/// Hibás HTML: nem zárt címsor, nem zárt idézőjel, nem zárt `<script>`; nagybetűs kiterjesztés.
const BROKEN: &str = "<h1>Legacy notes\n<p title=\"never closed\n<script>oops <h2>not a heading\n";

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "callgraph-html-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("docs")).unwrap();
        let project = Self(root.canonicalize().unwrap());
        fs::write(project.0.join("docs/design.html"), DESIGN).unwrap();
        fs::write(project.0.join("docs/LEGACY.HTM"), BROKEN).unwrap();
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
fn html_outline_lists_the_title_and_headings_with_line_ranges() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["show", "docs/design.html"], graph);
        for line in [
            "document design.html  L1-19\n  <title> Checkout redesign  L4-4\n  # Overview  L9-19\n",
            "\n    ## Goals  L11-13\n",
            "\n    ## Risks  L14-19\n",
        ] {
            assert!(out.contains(line), "{graph} {line:?}: {out}");
        }
        // a kép a szakasza alatt, hivatkozásként
        assert!(out.contains("flow.png"), "{graph}: {out}");
        assert!(!out.contains("no language registered"), "{graph}: {out}");
        // a <script>-be írt címsor nem címsor
        assert!(!out.contains("Fake"), "{graph}: {out}");
    }
}

#[test]
fn html_lines_prints_the_numbered_source_lines() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["show", "docs/design.html", "--lines", "9-10"], graph);
        assert!(out.contains("L9-10"), "{graph}: {out}");
        assert!(out.contains("<h1>Overview</h1>"), "{graph}: {out}");
        assert!(
            out.contains("The payment flow needs a refund button."),
            "{graph}: {out}"
        );
        assert!(!out.contains("<h2>Goals</h2>"), "{graph}: {out}");
    }
}

#[test]
fn html_strings_search_visible_text_and_text_attributes() {
    let project = Project::new();
    for graph in [true, false] {
        for target in ["docs/design.html", "."] {
            let out = project.run(&["show", target, "--strings", "refund"], graph);
            for want in [
                "\"The payment flow needs a refund button.\"",
                "\"Search refunds\"",
                "\"Refund search\"",
                "\"Refund flow diagram\"",
                "design.html::Goals",
                "design.html::Overview",
            ] {
                assert!(out.contains(want), "{graph} {target} {want}: {out}");
            }
            assert!(out.contains("L10 "), "{graph} {target}: {out}");
            assert!(out.contains("L13 "), "{graph} {target}: {out}");
            // a tördelt bekezdés egy szövegként, a szóközei egyre vonva
            let wrapped = project.run(&["show", target, "--strings", "gateway"], graph);
            assert!(
                wrapped.contains("\"Legacy gateway timeouts\""),
                "{graph} {target}: {wrapped}"
            );
        }
    }
}

#[test]
fn html_script_style_and_comment_content_is_not_a_string_match() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["show", ".", "--strings", "secret"], graph);
        assert!(out.starts_with("(no matches)"), "{graph}: {out}");
        // a nem szöveges attribútum (src) sem
        let out = project.run(&["show", ".", "--strings", "flow.png"], graph);
        assert!(out.starts_with("(no matches)"), "{graph}: {out}");
    }
}

#[test]
fn broken_html_is_shown_without_panic() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.command(&["show", "docs/LEGACY.HTM"], graph);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{graph}: {stderr}");
        assert!(!stderr.contains("panicked"), "{graph}: {stderr}");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("# Legacy notes  L1-3"), "{graph}: {text}");
        assert!(!text.contains("not a heading"), "{graph}: {text}");
        let lines = project.run(&["show", "docs/LEGACY.HTM", "--lines", "1-3"], graph);
        assert!(lines.contains("never closed"), "{graph}: {lines}");
    }
}

#[test]
fn locate_finds_an_html_document_by_its_file_name() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["locate", "design"], graph);
        assert!(out.contains("1. docs/design.html"), "{graph}: {out}");
    }
}
