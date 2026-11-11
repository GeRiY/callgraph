//! A Go frontend: regisztráció, bejárás (a `vendor` / `testdata` kimarad), és a `show` / `locate` kimenete
//! egy kis, két csomagos projekten.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use callgraph::io::FileWalker;
use callgraph::lang::{LanguageRegistry, LanguageRole};

const CART: &str = r#"package shop

import (
	"fmt"
	"example.com/app/pricing"
)

const TaxRate = 0.27

type Cart struct {
	items []int
}

func (c *Cart) Checkout(user string, coupon string) (bool, error) {
	total := pricing.ComputeTotal(c.Items(), TaxRate)
	fmt.Println("checkout", user)
	return total > 0, nil
}

func (c *Cart) Items() []int {
	return c.items
}
"#;

const CART_TEST: &str = r#"package shop

import "testing"

func TestCheckout(t *testing.T) {
	c := &Cart{}
	c.Checkout("u", "")
}
"#;

const PRICING: &str = r#"package pricing

func ComputeTotal(items []int, rate float64) float64 {
	return 0
}
"#;

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-go-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        fs::create_dir_all(&root).unwrap();
        let project = Self(root.canonicalize().unwrap());
        project.write("shop/cart.go", CART);
        project.write("shop/cart_test.go", CART_TEST);
        project.write("pricing/pricing.go", PRICING);
        project.write("Upper.GO", "package up\n\nfunc Up() {}\n");
        project.write("vendor/dep/dep.go", "package dep\n\nfunc Dep() {}\n");
        project.write(
            "shop/testdata/fixture.go",
            "package fixture\n\nfunc Fixture() {}\n",
        );
        project
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
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

#[test]
fn registry_routes_go_to_frontend() {
    let r = LanguageRegistry::with_defaults();
    for path in ["a/cart.go", "a/Cart.GO", "a/cart_test.go"] {
        let l = r.for_path(Path::new(path)).unwrap();
        assert_eq!((l.id(), l.role()), ("go", LanguageRole::Frontend), "{path}");
    }
    assert!(r.for_path(Path::new("a/go.mod")).is_none());
}

#[test]
fn walker_skips_vendor_and_testdata() {
    let project = Project::new();
    let r = LanguageRegistry::with_defaults();
    let files = FileWalker::for_registry(&r).collect(std::slice::from_ref(&project.0), &mut |_| {});
    let names: Vec<String> = files
        .iter()
        .filter_map(|f| f.file_name()?.to_str().map(str::to_string))
        .collect();
    for name in ["cart.go", "cart_test.go", "pricing.go", "Upper.GO"] {
        assert!(names.iter().any(|n| n == name), "{name}: {names:?}");
    }
    for name in ["dep.go", "fixture.go"] {
        assert!(!names.iter().any(|n| n == name), "{name}: {names:?}");
    }
}

#[test]
fn outline_body_and_signatures() {
    let project = Project::new();
    let outline = project.run(&["show", "shop/cart.go"]);
    for expected in ["class Cart", "TaxRate = 0.27", "Checkout()  L14-18"] {
        assert!(
            outline.contains(expected),
            "missing {expected:?} in:\n{outline}"
        );
    }
    let sigs = project.run(&["show", "shop/cart.go", "--signatures"]);
    assert!(
        sigs.contains("Checkout(user string, coupon string): (bool, error)  L14-18"),
        "{sigs}"
    );
    assert!(sigs.contains("Items(): []int  L20-22"), "{sigs}");
    let body = project.run(&["show", "shop/cart.go::Checkout", "--body"]);
    assert!(
        body.contains("total := pricing.ComputeTotal(c.Items(), TaxRate)"),
        "{body}"
    );
    let api = project.run(&["show", "pricing/pricing.go", "--format", "api"]);
    assert!(
        api.contains("ComputeTotal(items []int, rate float64): float64"),
        "{api}"
    );
}

#[test]
fn grep_finds_callers_across_packages() {
    let project = Project::new();
    let out = project.run(&["show", ".", "--grep", "ComputeTotal"]);
    assert!(out.contains("cart.go") && out.contains("Checkout"), "{out}");
    let strings = project.run(&["show", ".", "--strings", "checkout"]);
    assert!(strings.contains("checkout"), "{strings}");
}

#[test]
fn locate_indexes_go_names() {
    let project = Project::new();
    let out = project.run(&["locate", "ComputeTotal", "Cart"]);
    assert!(
        out.contains("pricing.go") && out.contains("cart.go"),
        "{out}"
    );
}
