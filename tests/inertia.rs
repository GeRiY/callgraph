//! Inertia-kötések egy kis Laravel + Inertia + Vue projekten (composer.json, package.json, controller,
//! `routes/web.php` a `Route::inertia`-val, `resources/js/app.js` az `import.meta.glob`-bal, három oldal): a
//! render-hívás page-fájlja az outline-ban, a page `rendered by:` sora, az `inertia` nézet (props-eltérés, hiányzó
//! és nem renderelt oldal), a `locate` page-indoklása; Inertia nélkül a régi kimenet.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("callgraph-inertia-{id}-{}-{n}", std::process::id()));
        for (path, body) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
        Self(root.canonicalize().unwrap())
    }

    fn run_with(&self, args: &[&str], graph: bool) -> String {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
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
        String::from_utf8(out.stdout).unwrap()
    }

    fn run(&self, args: &[&str]) -> String {
        self.run_with(args, true)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const COMPOSER_JSON: &str = r#"{
    "name": "acme/board",
    "require": {
        "php": "^8.2",
        "inertiajs/inertia-laravel": "^2.0",
        "laravel/framework": "^12.0"
    },
    "autoload": {"psr-4": {"App\\": "app/"}}
}
"#;

const COMPOSER_JSON_PLAIN: &str = r#"{
    "name": "acme/board",
    "require": {
        "php": "^8.2",
        "laravel/framework": "^12.0"
    },
    "autoload": {"psr-4": {"App\\": "app/"}}
}
"#;

const PACKAGE_JSON: &str = r#"{
    "private": true,
    "dependencies": {"@inertiajs/vue3": "^2.0", "vue": "^3.5"}
}
"#;

const PACKAGE_JSON_PLAIN: &str = r#"{
    "private": true,
    "dependencies": {"vue": "^3.5"}
}
"#;

const CONTROLLER: &str = r"<?php
namespace App\Http\Controllers;

use Inertia\Inertia;

class BoardController
{
    public function show()
    {
        return Inertia::render('Board', [
            'title' => 'Hello',
            'board' => $this->items(),
            'honorary' => true,
        ]);
    }

    public function index()
    {
        return inertia('Users/Index');
    }

    private function items(): array
    {
        return [];
    }
}
";

const ROUTES: &str = r"<?php

use Illuminate\Support\Facades\Route;

Route::inertia('/about', 'About');
";

const APP_JS: &str = r"import { createInertiaApp } from '@inertiajs/vue3'
import { resolvePageComponent } from 'laravel-vite-plugin/inertia-helpers'

createInertiaApp({
    resolve: (name) => resolvePageComponent(`./Pages/${name}.vue`, import.meta.glob('./Pages/**/*.vue')),
    setup({ el, App, props, plugin }) {
        return createApp({ render: () => h(App, props) }).use(plugin).mount(el)
    },
})
";

const BOARD_VUE: &str = r"<script setup>
defineProps({ title: String, board: Object })
</script>

<template>
    <h1>{{ title }}</h1>
</template>
";

const ABOUT_VUE: &str = "<template>\n    <p>About</p>\n</template>\n";
const OLD_VUE: &str = "<template>\n    <p>Old</p>\n</template>\n";

fn board(composer: &str, package: &str) -> Project {
    Project::new(&[
        ("composer.json", composer),
        ("package.json", package),
        ("app/Http/Controllers/BoardController.php", CONTROLLER),
        ("routes/web.php", ROUTES),
        ("resources/js/app.js", APP_JS),
        ("resources/js/Pages/Board.vue", BOARD_VUE),
        ("resources/js/Pages/About.vue", ABOUT_VUE),
        ("resources/js/Pages/Old.vue", OLD_VUE),
    ])
}

#[test]
fn outline_shows_the_page_file_of_each_render_call() {
    let project = board(COMPOSER_JSON, PACKAGE_JSON);
    let out = project.run(&["show", "app/Http/Controllers/BoardController.php"]);
    assert!(
        out.contains("Inertia\\Inertia::render → Pages/Board.vue"),
        "{out}"
    );
    assert!(
        out.contains("inertia → Pages/Users/Index.vue (missing)"),
        "{out}"
    );
    assert!(!out.contains("rendered by:"), "{out}");
    let routes = project.run(&["show", "routes/web.php"]);
    assert!(
        routes.contains("Route::inertia → Pages/About.vue"),
        "{routes}"
    );
    let json = project.run(&[
        "show",
        "app/Http/Controllers/BoardController.php",
        "--format",
        "json",
    ]);
    assert!(
        json.contains("\"inertia_page\": \"resources/js/Pages/Board.vue\""),
        "{json}"
    );
}

#[test]
fn page_outline_names_its_render_sites() {
    let project = board(COMPOSER_JSON, PACKAGE_JSON);
    for graph in [true, false] {
        let out = project.run_with(&["show", "resources/js/Pages/Board.vue"], graph);
        let second = out.lines().nth(1).unwrap_or_default();
        assert_eq!(second, "rendered by: BoardController::show L10", "{out}");
        let about = project.run_with(&["show", "resources/js/Pages/About.vue"], graph);
        assert!(about.contains("rendered by: routes/web.php L5"), "{about}");
        let old = project.run_with(&["show", "resources/js/Pages/Old.vue"], graph);
        assert!(!old.contains("rendered by:"), "{old}");
    }
}

#[test]
fn inertia_view_lists_pages_with_props_mismatch_missing_and_unrendered() {
    let project = board(COMPOSER_JSON, PACKAGE_JSON);
    let expected = "Pages/About.vue  ← routes/web.php L5\n\
Pages/Board.vue  ← BoardController::show L10 · props: title, board, honorary  ! props not declared: honorary\n\
Pages/Users/Index.vue  ← BoardController::index L19  ! page file missing\n\
unrendered: Pages/Old.vue\n";
    for graph in [true, false] {
        let out = project.run_with(&["show", ".", "--format", "inertia"], graph);
        assert_eq!(out, expected, "graph: {graph}");
    }
    // a hatókör: csak a route-fájl render-helye
    let routes = project.run(&["show", "routes", "--format", "inertia"]);
    assert!(
        routes.starts_with("Pages/About.vue  ← routes/web.php L5\n"),
        "{routes}"
    );
    assert!(!routes.contains("Pages/Board.vue  ←"), "{routes}");
}

#[test]
fn locate_gives_the_render_site_a_page_reason() {
    let project = board(COMPOSER_JSON, PACKAGE_JSON);
    let out = project.run(&["locate", "Board", "--all"]);
    assert!(out.contains("page ← BoardController::show"), "{out}");
    assert!(out.contains("resources/js/Pages/Board.vue"), "{out}");
    let users = project.run(&["locate", "Users/Index", "--all"]);
    assert!(users.contains("page ← BoardController::index"), "{users}");
}

#[test]
fn without_inertia_the_output_is_unchanged() {
    let project = board(COMPOSER_JSON_PLAIN, PACKAGE_JSON_PLAIN);
    let out = project.run(&["show", "app/Http/Controllers/BoardController.php"]);
    assert!(!out.contains(" → Pages/"), "{out}");
    let page = project.run(&["show", "resources/js/Pages/Board.vue"]);
    assert!(!page.contains("rendered by:"), "{page}");
    let view = project.run(&["show", ".", "--format", "inertia"]);
    assert_eq!(view, "(no Inertia page matches)\n");
    let located = project.run(&["locate", "Board", "--all"]);
    assert!(!located.contains("page ←"), "{located}");
}
