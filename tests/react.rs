//! React / JSX: component props (outline and the `props` view), custom hooks and context keys, react-router tables
//! and links, Inertia + React pages. Synthetic projects: `tests/fixtures/web/react_app` (React + TS + router) and
//! `tests/fixtures/web/laravel_react_app` (Laravel + Inertia + React).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// A temporary copy of a sample project (the cache goes into the copy, never into the repo).
struct Fixture(PathBuf);

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn temp_root() -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("callgraph-react-{id}-{}-{n}", std::process::id()))
}

impl Fixture {
    fn sample(name: &str) -> Self {
        let root = temp_root();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/web")
            .join(name);
        copy_dir(&source, &root);
        Self(root.canonicalize().unwrap())
    }

    /// A project written from `(path, text)` pairs.
    fn files(files: &[(&str, &str)]) -> Self {
        let root = temp_root();
        for (path, text) in files {
            let target = root.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, text).unwrap();
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
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
            .output()
            .expect("callgraph runs");
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn r8_outline_lists_the_props_of_a_component_under_it() {
    let fx = Fixture::sample("react_app");
    let card = fx.run(&["show", "src/components/Card.tsx"]);
    assert!(
        card.contains("props (4): title*, size, onSave*, children"),
        "{card}"
    );
    // propTypes: `isRequired` is required, a defaultProps entry is optional
    let row = fx.run(&["show", "src/components/Row.jsx"]);
    assert!(row.contains("props (3): label*, value, unit"), "{row}");
    assert!(!row.contains("…"), "{row}");
    // an imported props type is read; HTML attributes (`extends ButtonHTMLAttributes`) leave the set open
    let badge = fx.run(&["show", "src/components/Badge.tsx"]);
    assert!(badge.contains("props (2): label*, tone"), "{badge}");
    let button = fx.run(&["show", "src/components/Button.tsx"]);
    assert!(button.contains("props (1): variant*, …"), "{button}");
    // the facts are not listed as constants
    assert!(
        !card.contains("react_prop") && !card.contains("constants ("),
        "{card}"
    );
}

#[test]
fn r8_props_view_flags_missing_and_unknown_props_at_jsx_uses() {
    let out = Fixture::sample("react_app").run(&["show", "src", "--format", "props"]);
    assert!(out.starts_with("props: "), "{out}");
    let head = out.lines().next().unwrap();
    assert!(head.contains("3 missing required"), "{head}");
    assert!(head.contains("2 unknown props"), "{head}");
    assert!(out.contains("missing required prop 'onSave'"), "{out}");
    assert!(out.contains("missing required prop 'variant'"), "{out}");
    assert!(out.contains("missing required prop 'label'"), "{out}");
    assert!(out.contains("unknown prop 'colour'"), "{out}");
    assert!(out.contains("unknown prop 'extra'"), "{out}");
    assert!(out.contains("src/pages/Home.tsx L"), "{out}");
}

#[test]
fn r8_props_view_has_no_false_positives() {
    let out = Fixture::sample("react_app").run(&["show", "src", "--format", "props"]);
    // `{...spread}` suppresses "missing"; `...rest` / HTMLAttributes suppress "unknown"; `key` / `data-*` and the
    // `children` of a JSX body are fine; a default / `defaultProps` prop is optional
    for quiet in [
        "'onClick'",
        "'disabled'",
        "'key'",
        "'data-id'",
        "'unit'",
        "'children'",
        "'title'",
        "'size'",
        "'tone'",
        "'lineId'",
    ] {
        assert!(!out.contains(quiet), "{quiet}: {out}");
    }
    // the two `<Card>` findings are the broken one only, not the spread one
    let card: Vec<&str> = out.lines().filter(|l| l.ends_with("<Card>")).collect();
    assert_eq!(card.len(), 2, "{out}");
    assert!(
        out.contains("{...spread}: required props not checked"),
        "{out}"
    );
}

#[test]
fn r8_custom_hooks_are_marked_and_their_callers_resolve_to_the_file() {
    let fx = Fixture::sample("react_app");
    let hook = fx.run(&["show", "src/hooks/useCart.ts"]);
    assert!(
        hook.contains("useCart()") && hook.contains("[hook]"),
        "{hook}"
    );
    let home = fx.run(&["show", "src/pages/Home.tsx"]);
    assert!(home.contains("composable_use"), "{home}");
    assert!(home.contains("src/hooks/useCart.ts::useCart"), "{home}");
    let callers = fx.run(&["show", "src", "--grep", "src/hooks/useCart.ts::useCart$"]);
    assert!(
        callers.contains("Home.tsx") && callers.contains("Cart.tsx"),
        "{callers}"
    );
}

#[test]
fn r8_context_provider_and_consumers_share_one_key() {
    let out = Fixture::sample("react_app").run(&[
        "show",
        "src",
        "--grep",
        "react::context:src/context/CartContext.tsx::CartContext$",
    ]);
    assert!(out.contains("CartContext.tsx"), "{out}");
    assert!(out.contains("CartProvider"), "{out}");
    assert!(out.contains("useCart.ts"), "{out}");
}

#[test]
fn r8_routes_view_lists_react_router_tables_and_flags_unknown_link_paths() {
    let out = Fixture::sample("react_app").run(&["show", "src", "--format", "routes"]);
    // a JSX tree (nested, params) and an array table with a lazy import
    assert!(out.contains("/ → src/pages/Home.tsx"), "{out}");
    assert!(out.contains("/cart → src/pages/Cart.tsx"), "{out}");
    assert!(out.contains("/cart/:lineId → src/pages/Cart.tsx"), "{out}");
    assert!(out.contains("/about → src/pages/About.tsx"), "{out}");
    // literal `<Link to>` / `navigate()` paths against the table
    assert!(out.contains("unknown route path '/nowhere'"), "{out}");
    assert!(out.contains("unknown route path '/gone'"), "{out}");
    assert!(!out.contains("unknown route path '/cart'"), "{out}");
    assert!(out.contains("src/pages/Home.tsx L"), "{out}");
}

#[test]
fn r8_no_route_warning_without_a_route_table_or_with_a_dynamic_path() {
    let nav = "import { Link, useNavigate } from 'react-router-dom';\nexport function Nav() {\n  const go = useNavigate();\n  go('/x');\n  return <Link to=\"/y\">y</Link>;\n}\n";
    let none = Fixture::files(&[("src/Nav.tsx", nav)]).run(&["show", "src", "--format", "routes"]);
    assert!(!none.contains("unknown route path"), "{none}");
    let dynamic = "import { Routes, Route } from 'react-router-dom';\nimport Nav from './Nav';\nexport const base = '/b';\nexport const App = () => <Routes><Route path={base} element={<Nav />} /><Route path=\"/a\" element={<Nav />} /></Routes>;\n";
    let out = Fixture::files(&[("src/Nav.tsx", nav), ("src/App.tsx", dynamic)])
        .run(&["show", "src", "--format", "routes"]);
    assert!(out.contains("/a → src/Nav.tsx"), "{out}");
    assert!(!out.contains("unknown route path"), "{out}");
}

#[test]
fn r8_props_view_is_silent_for_wrapped_open_and_untyped_components() {
    let files = [
        (
            "src/Plain.jsx",
            "export default function Plain(props) { return <p>{props.text}</p>; }\n",
        ),
        (
            "src/Spread.tsx",
            "import type { ComponentProps } from 'react';\ntype P = ComponentProps<'button'> & { tone: string };\nexport function Spread({ tone, ...rest }: P) { return <button {...rest} data-tone={tone} />; }\n",
        ),
        (
            "src/Wrapped.tsx",
            "import { connect } from 'react-redux';\nfunction Inner({ a }: { a: string }) { return <i>{a}</i>; }\nexport default connect()(Inner);\n",
        ),
        (
            "src/Use.tsx",
            "import Plain from './Plain';\nimport { Spread } from './Spread';\nimport Wrapped from './Wrapped';\nexport const Use = () => <div><Plain text=\"a\" other=\"b\" /><Spread tone=\"x\" onClick={f} id=\"i\" /><Wrapped /></div>;\n",
        ),
    ];
    let out = Fixture::files(&files).run(&["show", "src/Use.tsx", "--format", "props"]);
    assert!(out.contains("no findings"), "{out}");
    assert!(!out.contains('⚠'), "{out}");
}

#[test]
fn r8_inertia_react_page_props_trace_and_never_passed() {
    let fx = Fixture::sample("laravel_react_app");
    let view = fx.run(&["show", ".", "--format", "inertia"]);
    assert!(view.contains("Pages/Events/Show.tsx"), "{view}");
    assert!(view.contains("EventController"), "{view}");
    assert!(view.contains("never passed: stale"), "{view}");
    let page = fx.run(&["show", "resources/js/Pages/Events/Show.tsx"]);
    assert!(page.contains("rendered by:"), "{page}");
    assert!(page.contains("props (3): event*, can*, stale*"), "{page}");
    for target in ["route::events.show", "Pages/Events/Show.tsx"] {
        let trace = fx.run(&["trace", target]);
        assert!(trace.contains("Events/Show.tsx"), "{target}: {trace}");
        assert!(trace.contains("props: event, can"), "{target}: {trace}");
        assert!(trace.contains("uses Stamp"), "{target}: {trace}");
    }
    let props = fx.run(&["show", "resources/js", "--format", "props"]);
    assert!(props.contains("missing required prop 'label'"), "{props}");
}
