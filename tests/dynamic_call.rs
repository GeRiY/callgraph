//! A `dynamic_call` jelölés a kimeneteken át: outline (a kategória-lista végén), `--format json` / `stats`,
//! `--category`, és hogy a jelölés nem hívó-él (`--grep`, `--depth` lábléc). PHP, TS, Python egy kis projektben.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const BUILDER: &str = r#"<?php
namespace App\Builders;

class Builder
{
    public function join(string $table): void
    {
    }

    public function apply(string $joinMethod): void
    {
        $this->$joinMethod('favorites', 1);
        $this->join('users');
    }

    public function viaContainer($abstract)
    {
        return app($abstract);
    }

    public function plain(): void
    {
        $this->join('x');
    }
}
"#;

const CALLER: &str = r#"<?php
namespace App\Builders;

class Caller
{
    public function run(Builder $b): void
    {
        $b->apply('x');
    }
}
"#;

const DISPATCHER: &str = r#"<?php
namespace App\Builders;

class Dispatcher
{
    public function go(Builder $b, $fn): void
    {
        $b->apply('x');
        $fn(1);
    }
}
"#;

const CLEAN_PHP: &str = r#"<?php
namespace App\Builders;

class Clean
{
    public function run(Builder $b): void
    {
        $b->apply('x');
        app(Builder::class);
        app('cache');
        resolve(Builder::class);
        $this->app->make(Builder::class);
        call_user_func([$this, 'run'], $b);
        call_user_func('strlen', 'x');
        $x = new Builder();
        Builder::make();
    }
}
"#;

const MIDDLEWARE: &str = r#"<?php
namespace App\Builders;

use Closure;

class Middleware
{
    public function __construct(private readonly Builder $builder, private readonly Closure $cb)
    {
    }

    public function handle(Request $r, Closure $next)
    {
        $local = fn ($x) => $x;
        $local(1);
        ($this->builder)(1);
        return $next($r);
    }

    public function callback(?callable $done)
    {
        return $done(1);
    }
}
"#;

const APP_TS: &str = r#"export class Panel {
  run(actions: any, action: string) {
    actions[action]();
    actions['reset']();
  }
}

export async function load(name: string) {
  return import(`./locale/${name}.js`);
}
"#;

const CLEAN_TS: &str = r#"export function run(o: any) {
  o['x']();
  o.m();
  o?.m();
  fn();
  import('./a.js');
  require('fs');
  new Map();
}
"#;

const PLUGIN_PY: &str = r#"class Plugin:
    def load(self, name):
        return getattr(self, name)(1)


def dispatch(name):
    return globals()[name]()
"#;

const CLEAN_PY: &str = r#"def run(o):
    getattr(o, 'x')()
    o.m()
    print(1)
"#;

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-dyncall-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        let files = [
            ("src/Builder.php", BUILDER),
            ("src/Caller.php", CALLER),
            ("src/Dispatcher.php", DISPATCHER),
            ("src/Clean.php", CLEAN_PHP),
            ("src/Middleware.php", MIDDLEWARE),
            ("web/app.ts", APP_TS),
            ("web/clean.ts", CLEAN_TS),
            ("lib/plugin.py", PLUGIN_PY),
            ("lib/clean.py", CLEAN_PY),
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
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .output()
            .expect("callgraph runs");
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

/// A `category == "dynamic_call"` objektumok a JSON-fában, bárhol.
fn dynamic_objects(value: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("category").and_then(|c| c.as_str()) == Some("dynamic_call") {
                out.push(value.clone());
            }
            for v in map.values() {
                dynamic_objects(v, out);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                dynamic_objects(v, out);
            }
        }
        _ => {}
    }
}

fn dynamic_in_json(project: &Project, path: &str) -> Vec<serde_json::Value> {
    let text = project.run(&["show", path, "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{path}: invalid JSON ({e}):\n{text}"));
    let mut found = Vec::new();
    dynamic_objects(&value, &mut found);
    found
}

#[test]
fn php_outline_lists_dynamic_call_last_in_the_method() {
    let project = Project::new();
    let out = project.run(&["show", "src/Builder.php"]);
    assert!(
        out.contains("    dynamic_call      $this->$joinMethod"),
        "{out}"
    );
    // a kategória-lista végén: a név szerint előbb álló `dynamic_call` mégis az `internal_helper` után jön
    let apply = &out[out.find("apply(").expect("apply method")..];
    let helper = apply.find("internal_helper").expect("internal_helper line");
    let dynamic = apply.find("dynamic_call").expect("dynamic_call line");
    assert!(helper < dynamic, "{apply}");
    assert!(out.contains("dynamic_call      app($abstract)"), "{out}");
}

#[test]
fn ts_and_python_outlines_list_dynamic_calls() {
    let project = Project::new();
    let ts = project.run(&["show", "web/app.ts"]);
    assert!(ts.contains("dynamic_call      actions[action]"), "{ts}");
    assert!(
        ts.contains("dynamic_call      import(`./locale/${name}.js`)"),
        "{ts}"
    );
    let py = project.run(&["show", "lib/plugin.py"]);
    assert!(py.contains("dynamic_call      getattr(self, name)"), "{py}");
    assert!(py.contains("dynamic_call      globals()[name]"), "{py}");
}

#[test]
fn literal_and_ordinary_calls_are_not_marked() {
    let project = Project::new();
    for file in [
        "src/Clean.php",
        "src/Middleware.php",
        "web/clean.ts",
        "lib/clean.py",
    ] {
        let out = project.run(&["show", file]);
        assert!(!out.contains("dynamic_call"), "{file}:\n{out}");
    }
    let all = project.run(&["show", "src", "web", "lib", "--category", "dynamic_call"]);
    for clean in ["Clean", "Middleware", "clean.ts", "clean.py"] {
        assert!(!all.contains(clean), "{clean} in:\n{all}");
    }
}

#[test]
fn category_filter_lists_only_the_markers() {
    let project = Project::new();
    let out = project.run(&["show", "src", "web", "lib", "--category", "dynamic_call"]);
    for expected in [
        "$this->$joinMethod",
        "app($abstract)",
        "$fn",
        "actions[action]",
        "getattr(self, name)",
        "globals()[name]",
    ] {
        assert!(out.contains(expected), "{expected} in:\n{out}");
    }
    // ordinary calls of the same methods are filtered out
    assert!(!out.contains("internal_helper"), "{out}");
}

#[test]
fn json_carries_kind_dynamic_and_no_resolution() {
    let project = Project::new();
    let found = dynamic_in_json(&project, "src/Builder.php");
    assert_eq!(found.len(), 2, "{found:?}");
    let join = found
        .iter()
        .find(|c| c["target"] == "$this->$joinMethod")
        .expect("joinMethod marker");
    assert_eq!(join["type"], "dynamic");
    assert!(join.get("resolved").is_none_or(|r| r.is_null()), "{join}");
    assert_eq!(join["caller"], "apply");
    assert_eq!(join["line"], 12);
    let ts = dynamic_in_json(&project, "web/app.ts");
    assert_eq!(ts.len(), 2, "{ts:?}");
    assert!(ts.iter().all(|c| c["type"] == "dynamic"));
    let py = dynamic_in_json(&project, "lib/plugin.py");
    assert_eq!(py.len(), 2, "{py:?}");
    assert!(py.iter().all(|c| c["type"] == "dynamic"));
}

#[test]
fn stats_counts_the_markers_as_a_category() {
    let project = Project::new();
    let out = project.run(&["show", "src/Builder.php", "--format", "stats"]);
    assert!(out.contains("by category:"), "{out}");
    assert!(
        out.lines()
            .any(|line| line.trim_start().starts_with("2\t") && line.ends_with("\tdynamic_call")),
        "{out}"
    );
    // the marker is counted in the total as well: join x2 + 2 markers + app() + ...
    let clean = project.run(&["show", "src/Clean.php", "--format", "stats"]);
    assert!(!clean.contains("dynamic_call"), "{clean}");
}

#[test]
fn mermaid_and_dot_label_the_marker_edge() {
    let project = Project::new();
    let mermaid = project.run(&["show", "src/Builder.php", "--format", "mermaid"]);
    assert!(mermaid.contains("dynamic_call"), "{mermaid}");
    let dot = project.run(&["show", "src/Builder.php", "--format", "dot"]);
    assert!(dot.contains("label=\"dynamic_call\""), "{dot}");
}

#[test]
fn grep_does_not_find_the_markers() {
    let project = Project::new();
    // (minta, fájl, amelyik csak a jelölés miatt szerepelne a találatok közt)
    for (pattern, file) in [
        ("joinMethod", "Builder.php"),
        (r"\$this->\$joinMethod", "Builder.php"),
        (r"app\(\$abstract\)", "Builder.php"),
        ("actions", "app.ts"),
        (r"actions\[action\]", "app.ts"),
    ] {
        let out = project.run(&["show", "src", "web", "lib", "--grep", pattern]);
        assert!(!out.contains(file), "{pattern}:\n{out}");
        assert!(!out.contains("dynamic_call"), "{pattern}:\n{out}");
    }
    // a jelölés mellett lévő közönséges hívás (a belső getattr / globals) viszont megmarad
    let out = project.run(&["show", "lib", "--grep", "getattr"]);
    assert!(out.contains("plugin.py"), "{out}");
    assert!(!out.contains("dynamic_call"), "{out}");
}

#[test]
fn grep_on_the_ordinary_call_of_the_same_method_still_works() {
    let project = Project::new();
    let out = project.run(&["show", "src", "--grep", "Builder::join"]);
    assert!(
        out.contains("Builder::apply") || out.contains("apply"),
        "{out}"
    );
    assert!(out.contains("plain"), "{out}");
    assert!(!out.contains("dynamic_call"), "{out}");
}

#[test]
fn depth_footer_counts_callers_with_dynamic_calls() {
    let project = Project::new();
    let out = project.run(&["show", "src", "--grep", "Builder::join", "--depth", "2"]);
    let footer = "contain dynamic calls — check them by hand)";
    // 1. szint: apply (dinamikus) és plain; 2. szint: Caller::run, Dispatcher::go (dinamikus) és Clean::run
    assert_eq!(out.matches(footer).count(), 2, "{out}");
    assert!(
        out.contains("(1 of these callers contain dynamic calls — check them by hand)"),
        "{out}"
    );
    // a jelölés nem hívó: nincs külön hívó-sor a dinamikus hívás miatt, és a kimenet nem tartalmazza a kategóriát
    assert!(!out.contains("dynamic_call"), "{out}");
}

#[test]
fn depth_without_dynamic_callers_has_no_footer() {
    let project = Project::new();
    // a Builder::make hívója (Clean::run) nem tartalmaz dinamikus hívást
    let out = project.run(&["show", "src", "--grep", "Builder::make", "--depth", "2"]);
    assert!(out.contains("Clean"), "{out}");
    assert!(!out.contains("dynamic calls"), "{out}");
}
