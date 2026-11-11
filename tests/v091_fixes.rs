//! A v0.9.1 javítások a valódi binárissal: JSX a `.js` fájlban és a komponens-használat hívásként, a Vue
//! `--format api` props/emits listája, a névterű `--grep` minta, a réteg-kapcsolók útvonal nélkül, a `🔒` a summary-ban.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-v091-it-{}-{id}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn raw(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
            .env_remove("CALLGRAPH_GITIGNORE")
            .output()
            .expect("callgraph runs")
    }

    /// Sikeres futás stdout-ja (a `\` jelek `/`-re cserélve).
    fn run(&self, args: &[&str]) -> String {
        let out = self.raw(args);
        assert!(
            out.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn show(&self, args: &[&str]) -> String {
        let root = self.0.to_string_lossy().into_owned();
        let mut all = vec!["show", "--project", root.as_str()];
        all.extend_from_slice(args);
        self.run(&all)
    }
}

const BUTTON: &str = "export function Button(props: { label: string }) {\n  return <button>{props.label}</button>;\n}\n";
const PAGE_TSX: &str = "import { Button } from './Button';\nexport function Page() {\n  return <section><Button label=\"a\" /></section>;\n}\n";

#[test]
fn plain_js_with_jsx_parses_without_a_syntax_error() {
    let project = Project::new();
    project.write(
        "plain.js",
        "export function render() { return <span>hi</span>; }\n",
    );
    project.write(
        "plain.mjs",
        "export function renderM() { return <span>hi</span>; }\n",
    );
    for file in ["plain.js", "plain.mjs"] {
        let outline = project.show(&[file]);
        assert!(!outline.contains("syntax error"), "{outline}");
        assert!(outline.contains("render"), "{outline}");
    }
}

#[test]
fn ts_files_stay_without_jsx_but_keep_type_assertions() {
    let project = Project::new();
    project.write(
        "cast.ts",
        "export function cast(x: unknown) { return <number>x; }\n",
    );
    let outline = project.show(&["cast.ts"]);
    assert!(
        !outline.contains("syntax error") && outline.contains("cast"),
        "{outline}"
    );
}

#[test]
fn jsx_component_use_is_a_call_in_tsx_and_js() {
    let project = Project::new();
    project.write("Button.tsx", BUTTON);
    project.write("Page.tsx", PAGE_TSX);
    project.write(
        "Home.js",
        "import { Button } from './Button';\nexport function Home() {\n  return <div><Button /></div>;\n}\n",
    );
    for file in ["Page.tsx", "Home.js"] {
        let outline = project.show(&[file]);
        assert!(
            outline.contains("component_use") && outline.contains("Button"),
            "{file}: {outline}"
        );
        // a kisbetűs elem nem hívás
        assert!(!outline.contains("section") && !outline.contains("div"), "{outline}");
    }
    let grep = project.show(&[".", "--grep", "Button$"]);
    assert!(grep.contains("Page") && grep.contains("Home"), "{grep}");
    let depth = project.show(&[".", "--grep", "Button$", "--depth", "2"]);
    assert!(depth.contains("Page") && depth.contains("Home"), "{depth}");
    // a `<section>` / `<div>` nem találat
    let root = project.0.to_string_lossy().into_owned();
    let none = project.raw(&[
        "show",
        "--project",
        &root,
        ".",
        "--grep",
        "^(section|div)$",
    ]);
    let text = String::from_utf8_lossy(&none.stdout);
    assert!(!text.contains("Page") && !text.contains("Home"), "{text}");
}

#[test]
fn vue_api_shows_props_and_emits() {
    let project = Project::new();
    project.write(
        "Comp.vue",
        "<script setup lang=\"ts\">\nconst props = defineProps<{ title: string }>();\nconst emit = defineEmits<{ (e: 'save', v: number): void }>();\nfunction go() { emit('save', 1); }\n</script>\n<template><div>{{ props.title }}</div></template>\n",
    );
    let api = project.show(&["Comp.vue", "--format", "api"]);
    assert!(api.contains("props (1):"), "{api}");
    assert!(api.contains("title: string [required]"), "{api}");
    assert!(api.contains("emits (1):"), "{api}");
    assert!(api.contains("    save\n"), "{api}");
    // más nyelv api-kimenete nem kap ilyen listát
    project.write(
        "a.ts",
        "export class A { public go(x: number): void {} }\n",
    );
    let other = project.show(&["a.ts", "--format", "api"]);
    assert!(!other.contains("props (") && !other.contains("emits ("), "{other}");
}

fn php_project() -> Project {
    let project = Project::new();
    project.write(
        "src/Svc/Real.php",
        "<?php\nnamespace App\\Svc;\n\nclass Real\n{\n    public static function m()\n    {\n        return 1;\n    }\n}\n",
    );
    project.write(
        "src/Caller.php",
        "<?php\nnamespace App;\n\nuse App\\Svc\\Real;\n\nclass Caller\n{\n    public function go()\n    {\n        return Real::m();\n    }\n}\n",
    );
    project
}

#[test]
fn namespaced_grep_pattern_matches_and_invalid_one_gets_a_hint() {
    let project = php_project();
    let found = project.show(&["src", "--grep", r"App\Svc\Real::m"]);
    assert!(found.contains("go"), "{found}");
    // a kétszerezett alak változatlanul működik
    let doubled = project.show(&["src", "--grep", r"App\\Svc\\Real::m"]);
    assert!(doubled.contains("go"), "{doubled}");
    // és a callers útvonal is
    let root = project.0.to_string_lossy().into_owned();
    let callers = project.run(&["callers", r"App\Svc\Real::m", "--project", &root]);
    assert!(callers.contains("go"), "{callers}");

    let bad = project.raw(&[
        "show",
        "--project",
        &root,
        "src",
        "--grep",
        r"app\models\User::find",
    ]);
    assert!(!bad.status.success());
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(stderr.contains("invalid pattern"), "{stderr}");
    assert!(
        stderr.contains('\u{1f4a1}') && stderr.contains("Real::m"),
        "{stderr}"
    );
}

#[test]
fn layer_switches_work_without_a_path() {
    let project = Project::new();
    project.write(
        "app/Services/OrderService.php",
        "<?php\nnamespace App\\Services;\n\nclass OrderService\n{\n    public function place()\n    {\n        return 1;\n    }\n}\n",
    );
    project.write(
        "app/Http/Controllers/OrderController.php",
        "<?php\nnamespace App\\Http\\Controllers;\n\nclass OrderController\n{\n    public function store()\n    {\n        return 2;\n    }\n}\n",
    );
    for args in [&["--layers"][..], &["--layer", "service"][..]] {
        let out = project.show(args);
        assert!(
            out.contains("OrderService") && !out.contains("callgraph show ["),
            "{args:?}: {out}"
        );
    }
    let by_layer = project.show(&["--grep", "place", "--by-layer"]);
    assert!(!by_layer.contains("callgraph show ["), "{by_layer}");
    // útvonallal a viselkedés változatlan
    let with_path = project.show(&["app", "--layers"]);
    assert!(with_path.contains("OrderService"), "{with_path}");
}

#[test]
fn protect_mark_shows_in_summary_but_not_in_machine_formats() {
    let project = Project::new();
    project.write(
        "migrations/m1.php",
        "<?php\nclass M1 { public function up() { return 1; } }\n",
    );
    project.write(
        "app/Shop.php",
        "<?php\nclass Shop { public function up() { return 2; } }\n",
    );
    project.write(".callgraphignore", "protect migrations/\n");
    let lock = '\u{1f512}';
    let summary = project.show(&["migrations/m1.php", "--format", "summary"]);
    assert!(
        summary.lines().any(|l| l.contains("M1") && l.ends_with(lock)),
        "{summary}"
    );
    let plain = project.show(&["app/Shop.php", "--format", "summary"]);
    assert!(plain.contains("Shop") && !plain.contains(lock), "{plain}");
    let both = project.show(&["migrations", "app", "--format", "summary"]);
    assert_eq!(both.matches(lock).count(), 1, "{both}");
    for machine in ["mermaid", "dot"] {
        let text = project.show(&["migrations/m1.php", "--format", machine]);
        assert!(!text.contains(lock), "{machine}: {text}");
    }
}
