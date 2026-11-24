//! A Python frontend: regisztráció, bejárás (a `venv` / `__pycache__` kimarad), és a `show` / `locate` kimenete
//! egy kis, két modulos projekten.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use callgraph::io::FileWalker;
use callgraph::lang::{LanguageRegistry, LanguageRole};

const SHOP: &str = r#""""Shop services."""
from pricing import compute_total
import logging

TAX_RATE = 0.27


class Cart(object):
    """A shopping cart."""

    @staticmethod
    def empty() -> "Cart":
        return Cart()

    async def checkout(self, user: str, coupon: str = "none") -> bool:
        total = compute_total(self.items(), TAX_RATE)
        logging.info("checkout %s", user)
        return total > 0

    def items(self):
        return []
"#;

const PRICING: &str = r#"def compute_total(items, rate: float = 0.0) -> float:
    return sum(items) * (1 + rate)
"#;

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-python-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        fs::create_dir_all(&root).unwrap();
        let project = Self(root.canonicalize().unwrap());
        project.write("shop.py", SHOP);
        project.write("pricing.py", PRICING);
        project.write("Upper.PY", "def up():\n    pass\n");
        project.write("venv/lib/site-packages/dep.py", "def dep():\n    pass\n");
        project.write("pkg/__pycache__/x.py", "def cached():\n    pass\n");
        project
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn run(&self, args: &[&str]) -> String {
        self.run_with(args, &[])
    }

    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .envs(env.iter().copied())
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

#[test]
fn registry_routes_python_to_frontend() {
    let r = LanguageRegistry::with_defaults();
    for path in ["a/shop.py", "a/Shop.PY", "a/types.pyi"] {
        let l = r.for_path(Path::new(path)).unwrap();
        assert_eq!(
            (l.id(), l.role()),
            ("python", LanguageRole::Frontend),
            "{path}"
        );
    }
    assert!(r.for_path(Path::new("a/shop.pyc")).is_none());
}

#[test]
fn walker_skips_venv_and_pycache() {
    let project = Project::new();
    let r = LanguageRegistry::with_defaults();
    let files = FileWalker::for_registry(&r).collect(std::slice::from_ref(&project.0), &mut |_| {});
    let names: Vec<String> = files
        .iter()
        .filter_map(|f| f.file_name()?.to_str().map(str::to_string))
        .collect();
    for name in ["shop.py", "pricing.py", "Upper.PY"] {
        assert!(names.iter().any(|n| n == name), "{name}: {names:?}");
    }
    for name in ["dep.py", "x.py"] {
        assert!(!names.iter().any(|n| n == name), "{name}: {names:?}");
    }
}

#[test]
fn outline_body_and_signatures() {
    let project = Project::new();
    let outline = project.run(&["show", "shop.py"]);
    for expected in [
        "class Cart extends object  L8-21",
        "TAX_RATE = 0.27",
        "checkout()  L15-18",
    ] {
        assert!(
            outline.contains(expected),
            "missing {expected:?} in:\n{outline}"
        );
    }
    let sigs = project.run(&["show", "shop.py", "--signatures"]);
    assert!(
        sigs.contains("checkout(user: str, coupon: str = \"none\"): bool  L15-18"),
        "{sigs}"
    );
    assert!(sigs.contains("empty(): \"Cart\""), "{sigs}");
    let body = project.run(&["show", "shop.py::checkout", "--body"]);
    assert!(
        body.contains("total = compute_total(self.items(), TAX_RATE)"),
        "{body}"
    );
    let api = project.run(&["show", "pricing.py", "--format", "api"]);
    assert!(
        api.contains("compute_total(items, rate: float = 0.0): float"),
        "{api}"
    );
}

#[test]
fn grep_finds_callers_across_modules() {
    let project = Project::new();
    let out = project.run(&["show", ".", "--grep", "compute_total"]);
    assert!(out.contains("shop.py") && out.contains("checkout"), "{out}");
    let strings = project.run(&["show", ".", "--strings", "checkout"]);
    assert!(strings.contains("checkout %s"), "{strings}");
}

#[test]
fn locate_indexes_python_names() {
    let project = Project::new();
    let out = project.run(&["locate", "compute_total", "Cart"]);
    assert!(
        out.contains("pricing.py") && out.contains("shop.py"),
        "{out}"
    );
}

const SCRIPT: &str = "import os\n\nclass Box:\n    def open(self):\n        return 1\n\n    def close(self):\n        return 2\n\n\ndef lone():\n    return 3\n\n\nprint(os.getcwd())\nprint(1)\nprint(2)\nprint(3)\nprint(4)\nprint(5)\n";

#[test]
fn body_respects_the_output_limit_and_full_overrides_it() {
    let project = Project::new();
    project.write("script.py", SCRIPT);
    let limited = project.run_with(
        &["show", "script.py::__module__", "--body"],
        &[("CALLGRAPH_OUT_LIMIT", "5")],
    );
    assert!(limited.contains("== "), "{limited}");
    assert!(limited.contains("⚠ --body output would be"), "{limited}");
    assert!(
        limited.contains("--lines A-B") && limited.contains("--full"),
        "{limited}"
    );
    assert!(limited.contains("module-level code"), "{limited}");
    assert!(!limited.contains("print(5)"), "{limited}");
    let full = project.run_with(
        &["show", "script.py::__module__", "--body", "--full"],
        &[("CALLGRAPH_OUT_LIMIT", "5")],
    );
    assert!(full.contains("print(5)") && !full.contains('⚠'), "{full}");
}

#[test]
fn outline_labels_module_level_code_without_parens() {
    let project = Project::new();
    project.write("script.py", SCRIPT);
    let outline = project.run(&["show", "script.py"]);
    assert!(outline.contains("  module-level code  L"), "{outline}");
    assert!(!outline.contains("__module__()"), "{outline}");
    assert!(outline.contains("open()"), "{outline}");
}

#[test]
fn qualified_and_comma_selectors_resolve() {
    let project = Project::new();
    project.write("script.py", SCRIPT);
    for selector in ["script.py::Box.open", "script.py::Box::open"] {
        let body = project.run(&["show", selector, "--body"]);
        assert!(
            body.contains("return 1") && !body.contains("return 2"),
            "{selector}: {body}"
        );
    }
    let both = project.run(&["show", "script.py::open,lone", "--body"]);
    assert!(
        both.contains("return 1") && both.contains("return 3"),
        "{both}"
    );
}
