//! Livewire: the component <-> view binding and its uses, the template <-> class contract (`wire:model`,
//! `wire:click`), events, the `livewire` view, the outline header lines and `trace livewire:x`. Synthetic project:
//! `tests/fixtures/web/livewire_app` (Livewire 3 components, a Form object, a full-page route, a Livewire 2 component).

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
    std::env::temp_dir().join(format!(
        "callgraph-livewire-{id}-{}-{n}",
        std::process::id()
    ))
}

impl Fixture {
    fn sample() -> Self {
        let root = temp_root();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/web/livewire_app");
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
fn lw_view_has_one_block_per_component() {
    let out = Fixture::sample().run(&["show", "app", "--format", "livewire"]);
    assert!(
        out.starts_with("livewire: 6 components, 7 uses, 3 warnings\n"),
        "{out}"
    );
    // class -> template (explicit render() and the default convention), uses and the full-page route
    assert!(
        out.contains("Counter  app/Livewire/Counter.php → livewire/counter.blade.php · used: 1\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "CreatePost  app/Livewire/CreatePost.php → livewire/create-post.blade.php · used: 1\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("UserTable  app/Livewire/Admin/UserTable.php → livewire/admin/user-table.blade.php · used: 1"),
        "{out}"
    );
    assert!(
        out.contains("Dashboard  app/Livewire/Dashboard.php → livewire/dashboard.blade.php · used: 1 · route GET /dashboard"),
        "{out}"
    );
    assert!(
        out.contains(
            "PostList  app/Livewire/PostList.php → livewire/post-list.blade.php · used: 2"
        )
    );
    assert!(out.contains(
        "LegacyBox  app/Http/Livewire/LegacyBox.php → livewire/legacy-box.blade.php · used: 1"
    ));
    // props (`#[Url]`), actions (computed and lifecycle methods are none), events, rules
    assert!(
        out.contains("  props: count, label · actions: increment, save\n"),
        "{out}"
    );
    assert!(
        out.contains("  props: search (url) · actions: refreshList, delete\n"),
        "{out}"
    );
    assert!(
        out.contains("  events: dispatches counter-changed, post-saved, counter-reset\n"),
        "{out}"
    );
    assert!(
        out.contains("  events: dispatches post-deleted · listens post-saved, refresh-list\n"),
        "{out}"
    );
    // the Livewire 2 `$listeners` map and `emit`
    assert!(
        out.contains("  events: dispatches legacy-ping · listens post-saved, refresh-list\n"),
        "{out}"
    );
    assert!(out.contains("  rules: count*\n"), "{out}");
    assert!(out.contains("  rules: role*\n"), "{out}");
    assert!(out.contains("  rules: form.title*, form.body\n"), "{out}");
}

#[test]
fn lw_contract_warnings_only_for_static_names_that_do_not_exist() {
    let out = Fixture::sample().run(&["show", "app", "--format", "livewire"]);
    assert!(
        out.contains("⚠ wire:click: no public method 'dec' in Counter  resources/views/livewire/counter.blade.php L6"),
        "{out}"
    );
    assert!(
        out.contains("⚠ wire:model: no public property 'cnt' in Counter  resources/views/livewire/counter.blade.php L4"),
        "{out}"
    );
    assert!(
        out.contains("⚠ wire:model: no public property 'titel' in PostForm  resources/views/livewire/create-post.blade.php L4"),
        "{out}"
    );
    assert_eq!(out.matches('⚠').count(), 3, "{out}");
    // `$refresh`, `$set`, `$toggle`, `$dispatch`, `$parent.x()`, existing methods and properties: no finding
    for quiet in [
        "$refresh",
        "'increment'",
        "'save'",
        "'label'",
        "'setRole'",
        "'delete'",
        "'toggle'",
        "'parent'",
    ] {
        assert!(!out.contains(quiet), "{quiet}: {out}");
    }
}

#[test]
fn lw_names_built_at_run_time_go_to_the_footer() {
    let fx = Fixture::sample();
    let code = fx.run(&["show", "app", "--format", "livewire"]);
    assert!(
        code.contains(
            "? 1 skipped: event name is a run-time value (app/Livewire/CreatePost.php L24)"
        ),
        "{code}"
    );
    let views = fx.run(&["show", "resources", "--format", "livewire"]);
    assert!(
        views.contains(
            "? 1 skipped: component name is a run-time value (resources/views/welcome.blade.php L6)"
        ),
        "{views}"
    );
    // the commented-out `<livewire:ghost />` is no use
    assert!(!views.contains("ghost"), "{views}");
}

#[test]
fn lw_outline_of_a_class_shows_its_view_and_uses() {
    let fx = Fixture::sample();
    let counter = fx.run(&["show", "app/Livewire/Counter.php"]);
    assert!(
        counter.contains("view: resources/views/livewire/counter.blade.php\n"),
        "{counter}"
    );
    assert!(
        counter.contains("used in: resources/views/welcome.blade.php L3\n"),
        "{counter}"
    );
    let list = fx.run(&["show", "app/Livewire/PostList.php"]);
    assert!(
        list.contains("used in: resources/views/livewire/dashboard.blade.php L3, resources/views/welcome.blade.php L5\n"),
        "{list}"
    );
    // the default convention (no render()) and the full-page route
    let create = fx.run(&["show", "app/Livewire/CreatePost.php"]);
    assert!(
        create.contains("view: resources/views/livewire/create-post.blade.php\n"),
        "{create}"
    );
    let dash = fx.run(&["show", "app/Livewire/Dashboard.php"]);
    assert!(dash.contains("used in: routes/web.php L9\n"), "{dash}");
}

#[test]
fn lw_outline_of_a_template_shows_its_class_calls_binds_and_findings() {
    let out = Fixture::sample().run(&["show", "resources/views/livewire/counter.blade.php"]);
    assert!(
        out.contains("livewire: Counter (app/Livewire/Counter.php)\n"),
        "{out}"
    );
    assert!(out.contains("calls: increment L5, save L12\n"), "{out}");
    assert!(out.contains("binds: label, cnt\n"), "{out}");
    assert!(
        out.contains("⚠ wire:click: no public method 'dec' in Counter  L6\n"),
        "{out}"
    );
    assert!(
        out.contains("⚠ wire:model: no public property 'cnt' in Counter  L4\n"),
        "{out}"
    );
    // a template of no component gets no Livewire lines
    let welcome = Fixture::sample().run(&["show", "resources/views/welcome.blade.php"]);
    assert!(!welcome.contains("livewire:"), "{welcome}");
}

#[test]
fn lw_event_key_finds_the_dispatcher_and_the_listeners() {
    let fx = Fixture::sample();
    let out = fx.run(&["show", "app", "--grep", "livewire::event:post-saved"]);
    for file in [
        "app/Livewire/Counter.php",
        "app/Livewire/CreatePost.php",
        "app/Livewire/PostList.php",
        "app/Http/Livewire/LegacyBox.php",
    ] {
        assert!(out.contains(file), "{file}: {out}");
    }
    // a template that dispatches: `wire:click="$dispatch('counter-reset')"`
    let views = fx.run(&[
        "show",
        "resources",
        "--grep",
        "livewire::event:counter-reset",
    ]);
    assert!(views.contains("livewire/counter.blade.php"), "{views}");
}

#[test]
fn lw_trace_of_a_component_and_of_a_full_page_component() {
    let fx = Fixture::sample();
    for target in ["livewire:counter", "Counter"] {
        let out = fx.run(&["trace", target]);
        assert!(
            out.starts_with("livewire Counter  app/Livewire/Counter.php:"),
            "{target}: {out}"
        );
        assert!(
            out.contains("view resources/views/livewire/counter.blade.php\n"),
            "{out}"
        );
        assert!(
            out.contains("used in 1: resources/views/welcome.blade.php L3\n"),
            "{out}"
        );
        assert!(out.contains("props count, label\n"), "{out}");
        assert!(out.contains("actions increment, save\n"), "{out}");
        assert!(out.contains("rules count*\n"), "{out}");
        assert!(
            out.contains("⚠ wire:click: no public method 'dec' in Counter"),
            "{out}"
        );
    }
    let dash = fx.run(&["trace", "livewire:dashboard"]);
    assert!(
        dash.contains("route GET /dashboard  name=dashboard  routes/web.php:9\n"),
        "{dash}"
    );
    assert!(dash.contains("models Post"), "{dash}");
    let nested = fx.run(&["trace", "livewire:admin.user-table"]);
    assert!(nested.contains("rules role*"), "{nested}");
    let unknown = fx.run(&["trace", "livewire:countr"]);
    assert!(unknown.contains("unknown target"), "{unknown}");
}

#[test]
fn lw_a_class_with_traits_is_never_checked_and_the_names_stay_silent() {
    let fx = Fixture::files(&[
        (
            "composer.json",
            "{\"require\": {\"livewire/livewire\": \"^3.0\"}}",
        ),
        (
            "app/Livewire/Feed.php",
            "<?php\nnamespace App\\Livewire;\nuse Livewire\\Component;\nuse Livewire\\WithPagination;\nclass Feed extends Component\n{\n    use WithPagination;\n    public function render() { return view('livewire.feed'); }\n}\n",
        ),
        (
            "resources/views/livewire/feed.blade.php",
            "<div wire:click=\"nextPage\"><i wire:model=\"page\"></i></div>\n",
        ),
    ]);
    let out = fx.run(&["show", "app", "--format", "livewire"]);
    assert!(
        out.contains("Feed  app/Livewire/Feed.php → livewire/feed.blade.php"),
        "{out}"
    );
    assert!(!out.contains('⚠'), "{out}");
}

#[test]
fn lw_projects_without_components_print_the_empty_message() {
    let fx = Fixture::files(&[("app/Plain.php", "<?php\nclass Plain {}\n")]);
    let out = fx.run(&["show", "app", "--format", "livewire"]);
    assert_eq!(out, "(no Livewire components found)\n");
}
