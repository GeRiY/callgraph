//! Szintaktikai hibás fájlok: a hiba mindig látszik (`⚠` sor a `FILE` alatt, záró sor a több fájlos / `--grep`
//! kimenetben, `parseErrors` a JSON-ban), a hiba előtti és utáni ép rész megmarad (JS / TS / Vue / Rust), a kilépési
//! kód 0 (`--strict` / `CALLGRAPH_STRICT=1`: 3).
//! A projekt-gráffal, gráf nélkül (`CALLGRAPH_NO_GRAPH=1`) és meleg gráffal (második futás) ugyanígy.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const TAIL: &str =
    "export function a() { return 1 }\nexport function b() { return a() }\nexport function c( {\n";
const PHP: &str = "<?php\nclass K {\n  public function ok() { return 1; }\n  public function broken( { return $x + ; }\n  public function after() { return 2; }\n}\n";
const RUST: &str = "fn a() -> u32 { 1 }\nfn b() -> u32 { a() }\nfn c( {\n";
const SWIFT: &str = "func a() -> Int { 1 }\nfunc b() -> Int { 2 }\nfunc c( {\n";
const VUE: &str = "<template><div/></template>\n<script setup>\nfunction a() { return 1 }\nfunction b( {\n</script>\n";
const GREP_A: &str = r"(^|::|\.|->)a$";

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let project = Self::empty();
        project.write("src/tail.js", TAIL);
        project.write("src/tail.ts", TAIL);
        project.write("src/k.php", PHP);
        project.write("src/r.rs", RUST);
        project.write("src/s.swift", SWIFT);
        project.write("src/v.vue", VUE);
        project.write("src/ok.js", "export function ok() { return 1 }\n");
        project
    }

    fn empty() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-syntax-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn write(&self, rel: &str, text: &str) {
        fs::write(self.0.join(rel), text).unwrap();
    }

    fn command(&self, args: &[&str], graph: bool) -> Output {
        self.command_env(args, graph, &[])
    }

    /// Mint a `command`, további környezeti változókkal.
    fn command_env(&self, args: &[&str], graph: bool, env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .env_remove("CALLGRAPH_STRICT")
            .env_remove("CALLGRAPH_NO_GRAPH");
        if !graph {
            command.env("CALLGRAPH_NO_GRAPH", "1");
        }
        command.envs(env.iter().copied());
        command.output().expect("callgraph runs")
    }

    /// Sikeres (0-s kilépési kódú) futás kimenete, `/`-es utakkal.
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

/// A gráffal (hideg, majd meleg futás) és gráf nélkül.
const MODES: [(bool, &str); 3] = [
    (true, "cold graph"),
    (true, "warm graph"),
    (false, "no graph"),
];

#[test]
fn js_and_ts_outline_keeps_the_intact_functions_and_shows_the_warning() {
    let project = Project::new();
    for (graph, mode) in MODES {
        for file in ["src/tail.js", "src/tail.ts"] {
            let out = project.run(&["show", file], graph);
            assert!(out.contains("  a()  L1-1"), "{mode} {file}: {out}");
            assert!(out.contains("  b()  L2-2"), "{mode} {file}: {out}");
            let file_line = out.lines().position(|l| l.starts_with("FILE  ")).unwrap();
            let warning = out.lines().nth(file_line + 1).unwrap_or_default();
            assert!(
                warning.starts_with("⚠ syntax error at L3: ")
                    && warning.ends_with(" — this file's outline may be incomplete"),
                "{mode} {file}: {out}"
            );
            // egyetlen fájlnál a FILE alatti sor elég: nincs záró sor
            assert!(!out.contains("files had syntax errors"), "{mode}: {out}");
            assert!(!out.contains("file had syntax errors"), "{mode}: {out}");
        }
    }
}

#[test]
fn method_of_a_broken_file_is_found_when_it_precedes_the_error() {
    let project = Project::new();
    for (graph, mode) in MODES {
        let out = project.run(&["show", "src/tail.js::a", "--body"], graph);
        assert!(out.contains("return 1"), "{mode}: {out}");
        // a --body nem mutatja a FILE alatti sort: a záró sor pótolja
        assert!(
            out.contains("⚠ 1 file had syntax errors (results may be incomplete): src/tail.js L3"),
            "{mode}: {out}"
        );
    }
}

#[test]
fn json_has_parse_errors_with_the_line() {
    let project = Project::new();
    for (graph, mode) in MODES {
        let out = project.run(&["show", "src/tail.js", "--format", "json"], graph);
        let rows: serde_json::Value = serde_json::from_str(&out).unwrap();
        let errors = &rows[0]["parseErrors"];
        assert_eq!(errors[0]["line"], 3, "{mode}: {out}");
        assert!(errors[0]["message"].as_str().is_some_and(|m| !m.is_empty()));
        let methods: Vec<&str> = rows[0]["methods"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["name"].as_str())
            .collect();
        assert!(
            methods.contains(&"a") && methods.contains(&"b"),
            "{mode}: {out}"
        );
        // ép fájlnál nincs parseErrors mező
        let ok = project.run(&["show", "src/ok.js", "--format", "json"], graph);
        assert!(!ok.contains("parseErrors"), "{mode}: {ok}");
    }
}

#[test]
fn grep_finds_calls_in_the_broken_file_and_prints_the_trailer() {
    let project = Project::new();
    for (graph, mode) in MODES {
        let out = project.run(&["show", ".", "--grep", GREP_A], graph);
        assert!(out.contains("src/tail.js"), "{mode}: {out}");
        assert!(out.contains("  b()  L2-2"), "{mode}: {out}");
        let trailer = out
            .lines()
            .find(|l| l.contains("files had syntax errors (results may be incomplete): "))
            .unwrap_or_else(|| panic!("{mode}: {out}"));
        assert!(
            trailer.starts_with("⚠ 6 files had syntax errors"),
            "{trailer}"
        );
        assert!(trailer.contains("src/tail.js L3"), "{trailer}");
        assert!(trailer.ends_with(", … and 1 more"), "{trailer}");
        // a mélységi nézet is jelzi
        let depth = project.run(&["show", ".", "--grep", GREP_A, "--depth", "2"], graph);
        assert!(depth.contains("files had syntax errors"), "{mode}: {depth}");
    }
}

#[test]
fn grep_without_a_match_still_warns_about_the_broken_files() {
    let project = Project::new();
    for (graph, mode) in MODES {
        let out = project.run(&["show", ".", "--grep", "nothing_calls_this_xyz"], graph);
        assert!(out.contains("⚠ 6 files had syntax errors"), "{mode}: {out}");
    }
}

#[test]
fn vue_rust_php_and_swift_files_show_the_warning() {
    let project = Project::new();
    for (graph, mode) in MODES {
        let vue = project.run(&["show", "src/v.vue"], graph);
        assert!(vue.contains("  a()  L3-3"), "{mode}: {vue}");
        assert!(vue.contains("⚠ syntax error at L4: "), "{mode}: {vue}");

        let rust = project.run(&["show", "src/r.rs"], graph);
        assert!(
            rust.contains("  a()  L1-1") && rust.contains("  b()  L2-2"),
            "{mode}: {rust}"
        );
        assert!(rust.contains("⚠ syntax error at L3: "), "{mode}: {rust}");

        let php = project.run(&["show", "src/k.php"], graph);
        assert!(
            php.contains("ok()") && php.contains("after()"),
            "{mode}: {php}"
        );
        assert!(php.contains("⚠ syntax error at L4: "), "{mode}: {php}");

        let swift = project.run(&["show", "src/s.swift"], graph);
        assert!(
            swift.contains("a()") && swift.contains("b()"),
            "{mode}: {swift}"
        );
        assert!(swift.contains("⚠ syntax error at L3: "), "{mode}: {swift}");
    }
}

#[test]
fn directory_summary_and_locate_print_the_trailer() {
    let project = Project::new();
    for (graph, mode) in MODES {
        let summary = project.run(&["show", "src", "--format", "summary"], graph);
        assert!(
            summary.contains("⚠ 6 files had syntax errors"),
            "{mode}: {summary}"
        );
        let outline = project.run(&["show", "src"], graph);
        assert!(
            outline.contains("⚠ 6 files had syntax errors"),
            "{mode}: {outline}"
        );
        let locate = project.run(&["locate", "tail"], graph);
        assert!(
            locate.contains("⚠ 6 files had syntax errors"),
            "{mode}: {locate}"
        );
    }
}

#[test]
fn valid_files_have_no_warning() {
    let project = Project::new();
    for (graph, mode) in MODES {
        let out = project.run(&["show", "src/ok.js"], graph);
        assert!(!out.contains('⚠'), "{mode}: {out}");
        let grep = project.run(&["show", "src/ok.js", "--grep", "ok"], graph);
        assert!(!grep.contains("syntax error"), "{mode}: {grep}");
    }
}

const BAD: &str = "export function ok() { return 1 }\nexport function broken( { return x + ; }\nexport function after() { return 2 }\n";
const TWO: &str = "export function a() { return 1 }\nexport function b( { return 2 }\nexport function c() { return 3 }\nexport function d( { return 4 }\nexport function e() { return 5 }\n";
const VUE_FIRST: &str = "<template><div/></template>\n<script setup>\nfunction a( { return 1 }\nfunction b() { return 2 }\nfunction c() { return 3 }\n</script>\n";
const MULTI: &str = "export function a() {\n  return 1\n}\n\nexport function b(x) {\n  return x + ;\n}\n\nexport function c() {\n  return 3\n}\n";

fn recovery_project() -> Project {
    let project = Project::empty();
    project.write("src/bad.js", BAD);
    project.write("src/bad.ts", BAD);
    project.write("src/two.js", TWO);
    project.write("src/first.vue", VUE_FIRST);
    project.write("src/multi.js", MULTI);
    project
}

#[test]
fn functions_after_the_error_are_kept_with_their_lines() {
    let project = recovery_project();
    for (graph, mode) in MODES {
        for file in ["src/bad.js", "src/bad.ts"] {
            let out = project.run(&["show", file], graph);
            assert!(out.contains("  ok()  L1-1"), "{mode} {file}: {out}");
            assert!(out.contains("  after()  L3-3"), "{mode} {file}: {out}");
            assert!(!out.contains("broken()"), "{mode} {file}: {out}");
            assert!(
                out.contains("⚠ syntax error at L2: "),
                "{mode} {file}: {out}"
            );
        }
        let two = project.run(&["show", "src/two.js"], graph);
        for (name, line) in [("a", 1), ("c", 3), ("e", 5)] {
            assert!(
                two.contains(&format!("  {name}()  L{line}-{line}")),
                "{mode}: {two}"
            );
        }
        assert!(
            !two.contains("  b()") && !two.contains("  d()"),
            "{mode}: {two}"
        );
        let multi = project.run(&["show", "src/multi.js"], graph);
        assert!(
            multi.contains("  a()  L1-3") && multi.contains("  c()  L9-11"),
            "{mode}: {multi}"
        );
        // a hiba utáni függvény törzse is az eredeti sorokból jön
        let body = project.run(&["show", "src/bad.js::after", "--body"], graph);
        assert!(body.contains("return 2"), "{mode}: {body}");
    }
}

#[test]
fn vue_error_in_the_first_function_keeps_the_later_ones() {
    let project = recovery_project();
    for (graph, mode) in MODES {
        let out = project.run(&["show", "src/first.vue"], graph);
        assert!(out.contains("  b()  L4-4"), "{mode}: {out}");
        assert!(out.contains("  c()  L5-5"), "{mode}: {out}");
        assert!(out.contains("⚠ syntax error at L3: "), "{mode}: {out}");
    }
}

#[test]
fn strict_mode_exits_with_3_on_syntax_errors_and_keeps_the_output() {
    let project = recovery_project();
    project.write("src/ok.js", "export function ok() { return 1 }\n");
    for (graph, mode) in MODES {
        let plain = project.run(&["show", "src/bad.js"], graph);
        let strict = project.command(&["show", "src/bad.js", "--strict"], graph);
        assert_eq!(strict.status.code(), Some(3), "{mode}");
        assert_eq!(
            String::from_utf8_lossy(&strict.stdout).replace('\\', "/"),
            plain
        );
        // env-ből is (CI)
        let strict_env = [("CALLGRAPH_STRICT", "1")];
        let env = project.command_env(&["show", ".", "--grep", "ok"], graph, &strict_env);
        assert_eq!(env.status.code(), Some(3), "{mode}");
        // ép fájl: 0 marad
        let ok = project.command(&["show", "src/ok.js", "--strict"], graph);
        assert_eq!(ok.status.code(), Some(0), "{mode}");
        for args in [
            &["show", "src", "--strict"][..],
            &["show", ".", "--grep", "ok", "--strict"],
            &["locate", "after", "--strict"],
        ] {
            let out = project.command(args, graph);
            assert_eq!(out.status.code(), Some(3), "{mode} {args:?}");
        }
        // a profil a projekt-gráfból tudja a hibás fájlokat (gráf nélkül csak a nevekből dolgozik)
        if graph {
            let out = project.command(&["profile", "--strict"], graph);
            assert_eq!(out.status.code(), Some(3), "{mode}");
            assert_eq!(project.command(&["profile"], graph).status.code(), Some(0));
        }
    }
}
