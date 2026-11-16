//! Blade-sablonok egy kis Laravel projekten (composer.json, route-ok, controller, layout + oldal + partial + anonim
//! komponens, nyelvfájl): a `view` container és a szakaszok sortartománnyal, a nézet-használat fájlra feloldva, a
//! `route('név')` link a route-táblához (és az ismeretlen név jelzése), a `rendered by:` / `used by:` sor, az i18n
//! kulcsok, a `--grep` a layout fájlnevére. Gráffal és gráf nélkül is.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("callgraph-blade-{id}-{}-{n}", std::process::id()));
        for (path, body) in FILES {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
        Self(root.canonicalize().unwrap())
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

const COMPOSER: &str = r#"{
    "name": "acme/users",
    "require": {"php": "^8.2", "laravel/framework": "^12.0"},
    "autoload": {"psr-4": {"App\\": "app/"}}
}
"#;

const ROUTES: &str = r"<?php

use App\Http\Controllers\UserController;
use Illuminate\Support\Facades\Route;

Route::get('/users', [UserController::class, 'index'])->name('users.index');
Route::post('/users', [UserController::class, 'store'])->name('users.store');
";

const CONTROLLER: &str = r"<?php
namespace App\Http\Controllers;

class UserController
{
    public function index()
    {
        return view('admin.users.index', ['users' => []]);
    }

    public function store()
    {
        return redirect()->route('users.index');
    }
}
";

const LAYOUT: &str = r"<!DOCTYPE html>
<html>
<head>
    <title>@yield('title')</title>
    @stack('scripts')
</head>
<body>
    @include('partials.nav')
    @yield('content')
</body>
</html>
";

const PAGE: &str = r#"@extends('layouts.app')

@section('title', 'Users')

@section('content')
    <h1>{{ __('auth.failed') }}</h1>
    <x-alert type="error" :message="$errors->first()" />
    <form method="POST" action="{{ route('users.store') }}">
        @csrf
        <a href="{{ route('users.nope') }}">{{ __('auth.missing') }}</a>
    </form>
    @foreach ($users as $user)
        <li>{{ $user->name }} {{ Str::limit($user->bio, 20) }}</li>
    @endforeach
@endsection
"#;

const NAV: &str = "<nav><a href=\"{{ route('users.index') }}\">Users</a></nav>\n";

const ALERT: &str = "@props(['type' => 'info', 'message'])\n<div class=\"alert alert-{{ $type }}\">{{ $message }}</div>\n";

const LANG: &str = "<?php\nreturn [\n    'failed' => 'These credentials do not match.',\n];\n";

const FILES: &[(&str, &str)] = &[
    ("composer.json", COMPOSER),
    ("routes/web.php", ROUTES),
    ("app/Http/Controllers/UserController.php", CONTROLLER),
    ("resources/views/layouts/app.blade.php", LAYOUT),
    ("resources/views/admin/users/index.blade.php", PAGE),
    ("resources/views/partials/nav.blade.php", NAV),
    ("resources/views/components/alert.blade.php", ALERT),
    ("lang/en/auth.php", LANG),
];

#[test]
fn page_outline_has_the_view_sections_and_resolved_uses() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(
            &["show", "resources/views/admin/users/index.blade.php"],
            graph,
        );
        for want in [
            "view admin.users.index extends layouts.app",
            "section:content  L5-15",
            "resources/views/layouts/app.blade.php",
            "resources/views/components/alert.blade.php",
            "view_use",
            "component_use",
        ] {
            assert!(out.contains(want), "{graph} {want}: {out}");
        }
        assert!(!out.contains("(no class"), "{graph}: {out}");
    }
}

#[test]
fn route_calls_link_to_the_route_table_and_unknown_names_are_flagged() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(
            &["show", "resources/views/admin/users/index.blade.php"],
            graph,
        );
        assert!(
            out.contains("route::users.store → POST /users · UserController@store"),
            "{graph}: {out}"
        );
        assert!(
            out.contains("route::users.nope ⚠ no route"),
            "{graph}: {out}"
        );
    }
    let ziggy = project.run(&["show", ".", "--format", "ziggy"], true);
    assert!(
        ziggy.contains("unknown route name 'users.nope'")
            && ziggy.contains("resources/views/admin/users/index.blade.php L10"),
        "{ziggy}"
    );
    assert!(
        !ziggy.contains("unknown route name 'users.store'"),
        "{ziggy}"
    );
}

#[test]
fn rendered_by_and_used_by_lines() {
    let project = Project::new();
    for graph in [true, false] {
        let page = project.run(
            &["show", "resources/views/admin/users/index.blade.php"],
            graph,
        );
        assert!(
            page.contains("rendered by: UserController::index L8"),
            "{graph}: {page}"
        );
        let layout = project.run(&["show", "resources/views/layouts/app.blade.php"], graph);
        assert!(
            layout.contains("used by: admin.users.index L1 @extends"),
            "{graph}: {layout}"
        );
        let alert = project.run(
            &["show", "resources/views/components/alert.blade.php"],
            graph,
        );
        assert!(
            alert.contains("used by: admin.users.index L7 <x-…>"),
            "{graph}: {alert}"
        );
        assert!(alert.contains("$type = 'info'"), "{graph}: {alert}");
    }
}

#[test]
fn grep_finds_the_views_extending_a_layout() {
    let project = Project::new();
    let out = project.run(&["show", ".", "--grep", "layouts/app.blade.php"], true);
    assert!(out.contains("index.blade.php"), "{out}");
}

#[test]
fn translation_keys_of_the_templates_are_checked_against_the_lang_files() {
    let project = Project::new();
    let out = project.run(
        &["show", "resources/views/admin/users/index.blade.php"],
        true,
    );
    assert!(out.contains("i18n::auth.failed"), "{out}");
    assert!(out.contains("i18n::auth.missing"), "{out}");
}
