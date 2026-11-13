//! A Laravel-kötések egységtesztjei memóriabeli fájlrendszeren (`ProjectFiles`): a route-tábla (lánc-név, csoportok,
//! resource-ok, invokálható controller, `Route::inertia`), az outline linkje (létező / hiányzó név, dinamikus
//! argumentum), a controller `route:` sora, a json-hoz a keresés és a `ziggy` nézet sorai.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::super::files::ProjectFiles;
use super::super::inertia::Inertia;
use super::routes::Action;
use super::{Laravel, NAME};
use crate::i18n::Lang;
use crate::model::{Call, FileGraph, Imports};
use crate::packages::{Ecosystem, EcosystemPackages, Package, Packages, ProjectPackages};

/// Memóriabeli fájlrendszer: a gyökérhez relatív út → szöveg.
pub(super) struct MemFiles(BTreeMap<PathBuf, String>);

impl MemFiles {
    pub(super) fn new(files: &[(&str, &str)]) -> Self {
        Self(
            files
                .iter()
                .map(|(rel, text)| (root().join(rel), (*text).to_owned()))
                .collect(),
        )
    }
}

impl ProjectFiles for MemFiles {
    fn read(&self, path: &Path) -> Option<String> {
        self.0.get(path).cloned()
    }

    fn files_under(&self, dir: &Path) -> Vec<PathBuf> {
        self.0
            .keys()
            .filter(|path| path.starts_with(dir))
            .cloned()
            .collect()
    }
}

pub(super) fn root() -> PathBuf {
    PathBuf::from("/p")
}

pub(super) fn graph(
    file: &str,
    module: Option<&str>,
    imports: &[(&str, &str)],
    calls: Vec<Call>,
) -> FileGraph {
    let mut map = Imports::default();
    for (alias, full) in imports {
        map.insert((*alias).to_owned(), (*full).to_owned());
    }
    FileGraph {
        file: file.into(),
        module: module.map(str::to_owned),
        framework: None,
        namespace: None,
        imports: map,
        containers: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        methods: Vec::new(),
        calls,
        comments: Vec::new(),
        control_flow: Vec::new(),
        strings: Vec::new(),
        writes: Vec::new(),
        sfc: None,
        binary: false,
        parse_errors: Vec::new(),
    }
}

pub(super) fn call(
    kind: &str,
    target: &str,
    resolved: Option<&str>,
    category: &str,
    line: u32,
) -> Call {
    Call {
        container: None,
        caller: Some("__top__".into()),
        line,
        end_line: None,
        kind: kind.into(),
        target: target.into(),
        resolved: resolved.map(str::to_owned),
        on: None,
        category: category.into(),
        args: None,
        handler: None,
        from: None,
    }
}

/// Egy JS-oldali `route('név')` hívás, ahogy a gráf tárolja (a `resolved`: `route::név`).
pub(super) fn js_route(name: &str, line: u32) -> Call {
    call(
        "func_call",
        "route",
        Some(&format!("route::{name}")),
        "route_helper",
        line,
    )
}

const CONTROLLERS: &str = "App\\Http\\Controllers";

/// A `routes/web.php` szövegéből a tábla (a fájl `use`-ai az `imports`-ban).
pub(super) fn table(text: &str, imports: &[(&str, &str)]) -> Laravel {
    let files = MemFiles::new(&[("routes/web.php", text)]);
    let web = graph("/p/routes/web.php", None, imports, Vec::new());
    let path = root().join("routes/web.php");
    let mut laravel = Laravel::new(&root());
    laravel.scan([(path.as_path(), &web)], &files, false);
    laravel
}

/// (név, ige, uri, kezelő) soronként — a tesztek ezt hasonlítják.
fn rows(laravel: &Laravel) -> Vec<(String, String, String, String)> {
    laravel
        .routes()
        .iter()
        .map(|r| {
            (
                r.name.clone().unwrap_or_default(),
                r.verb.clone(),
                r.uri.clone(),
                r.action.label(),
            )
        })
        .collect()
}

fn row(name: &str, verb: &str, uri: &str, action: &str) -> (String, String, String, String) {
    (name.into(), verb.into(), uri.into(), action.into())
}

const WEB: &str = r#"<?php
use App\Http\Controllers\Auth\RegisteredUserController;
use Illuminate\Support\Facades\Route;

Route::middleware('guest')->group(function () {
    Route::get('register', [RegisteredUserController::class, 'create'])
        ->name('register');

    Route::post('register', [RegisteredUserController::class, 'store'])->name('register.store');
});
"#;

const WEB_IMPORTS: &[(&str, &str)] = &[
    (
        "RegisteredUserController",
        "App\\Http\\Controllers\\Auth\\RegisteredUserController",
    ),
    ("Route", "Illuminate\\Support\\Facades\\Route"),
];

#[test]
fn a_chained_name_and_the_imported_controller_class_make_a_route() {
    let laravel = table(WEB, WEB_IMPORTS);
    assert_eq!(
        rows(&laravel),
        vec![
            row(
                "register",
                "GET",
                "/register",
                "RegisteredUserController@create"
            ),
            row(
                "register.store",
                "POST",
                "/register",
                "RegisteredUserController@store"
            ),
        ]
    );
    let first = &laravel.routes()[0];
    assert_eq!(first.file, "routes/web.php");
    assert_eq!(first.line, 6);
    assert_eq!(
        first.action,
        Action::Controller {
            class: format!("{CONTROLLERS}\\Auth\\RegisteredUserController"),
            method: "create".into(),
        }
    );
    assert_eq!(laravel.routes()[1].line, 9);
}

#[test]
fn nested_groups_compose_prefix_name_and_controller() {
    let text = r#"<?php
use App\Http\Controllers\Admin\UserController;

Route::prefix('admin')->name('admin.')->middleware('auth')->group(function () {
    Route::controller(UserController::class)->prefix('users')->name('users.')->group(function () {
        Route::get('/', 'index')->name('index');
        Route::get('{user}', 'show')->name('show');
        Route::post('/', 'store');
    });
    // Route::get('commented', 'x')->name('commented');
    Route::get('dashboard', fn () => 1)->name('dash');
});
Route::get('/', function () { return Inertia::render('Welcome', ['a' => 1]); })->name('welcome');
"#;
    let laravel = table(
        text,
        &[(
            "UserController",
            "App\\Http\\Controllers\\Admin\\UserController",
        )],
    );
    assert_eq!(
        rows(&laravel),
        vec![
            row(
                "admin.users.index",
                "GET",
                "/admin/users",
                "UserController@index"
            ),
            row(
                "admin.users.show",
                "GET",
                "/admin/users/{user}",
                "UserController@show"
            ),
            // név nélküli route a névvel rendelkező csoportban: nincs neve
            row("", "POST", "/admin/users", "UserController@store"),
            row("admin.dash", "GET", "/admin/dashboard", "closure"),
            row("welcome", "GET", "/", "page Welcome"),
        ]
    );
}

#[test]
fn the_array_form_of_group_applies_prefix_as_and_controller() {
    let text = r#"<?php
Route::group(['prefix' => 'api/v1', 'as' => 'api.', 'controller' => \App\Http\Controllers\TagController::class], function () {
    Route::get('tags', 'index')->name('tags');
});
"#;
    let laravel = table(text, &[]);
    let routes = laravel.routes();
    assert_eq!(routes.len(), 1, "{routes:?}");
    assert_eq!(routes[0].name.as_deref(), Some("api.tags"));
    assert_eq!(routes[0].uri, "/api/v1/tags");
    assert_eq!(
        routes[0].action,
        Action::Controller {
            class: "App\\Http\\Controllers\\TagController".into(),
            method: "index".into(),
        }
    );
}

#[test]
fn resources_expand_and_honor_only_except_and_prefixes() {
    let text = r#"<?php
Route::resource('photos', PhotoController::class)->only(['index', 'show']);
Route::apiResource('/posts', PostController::class)->except('destroy');
Route::name('v1.')->prefix('v1')->resource('comments', CommentController::class);
Route::apiResource('categories', CategoryController::class)->only(['update']);
"#;
    let laravel = table(text, &[]);
    let rows = rows(&laravel);
    let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "photos.index",
            "photos.show",
            "posts.index",
            "posts.store",
            "posts.show",
            "posts.update",
            "v1.comments.index",
            "v1.comments.create",
            "v1.comments.store",
            "v1.comments.show",
            "v1.comments.edit",
            "v1.comments.update",
            "v1.comments.destroy",
            "categories.update",
        ]
    );
    assert_eq!(
        rows[1],
        row(
            "photos.show",
            "GET",
            "/photos/{photo}",
            "PhotoController@show"
        )
    );
    assert_eq!(
        rows[5],
        row(
            "posts.update",
            "PUT|PATCH",
            "/posts/{post}",
            "PostController@update"
        )
    );
    assert_eq!(
        rows[10],
        row(
            "v1.comments.edit",
            "GET",
            "/v1/comments/{comment}/edit",
            "CommentController@edit"
        )
    );
    assert_eq!(rows[13].2, "/categories/{category}");
}

#[test]
fn invokable_controllers_inertia_view_and_redirect_routes() {
    let text = r#"<?php
Route::get('verify-email', EmailVerificationPromptController::class)
    ->middleware(['signed', 'throttle:6,1'])
    ->name('verification.notice');
Route::inertia('/about', 'About')->name('about');
Route::view('/terms', 'legal.terms');
Route::redirect('/old', '/new');
Route::match(['get', 'post'], '/hook', [HookController::class, 'handle'])->name('hook');
Route::currentRouteName();
Route::get($dynamic, 'x');
"#;
    let laravel = table(text, &[]);
    assert_eq!(
        rows(&laravel),
        vec![
            row(
                "verification.notice",
                "GET",
                "/verify-email",
                "EmailVerificationPromptController@__invoke"
            ),
            row("about", "GET", "/about", "page About"),
            row("", "GET", "/terms", "view legal.terms"),
            row("", "ANY", "/old", "redirect /new"),
            row("hook", "GET|POST", "/hook", "HookController@handle"),
            // a nem literál uri: `?`, de a route létezik
            row("", "GET", "/?", "x@__invoke"),
        ]
    );
}

#[test]
fn unparseable_route_files_are_skipped_without_error() {
    for text in [
        "",
        "<?php Route::",
        "<?php Route::get(",
        "<?php Route::get('/x', [A::class, 'b'",
        "<?php Route::group([], function () { Route::get('/x'",
        "<?php Route::->name('x');",
        "<?php // Route::get('/x', 'a@b');\n",
    ] {
        assert!(table(text, &[]).routes().is_empty(), "{text}");
    }
    // a lezáratlan csoport előtti route még megvan
    let laravel = table("<?php\nRoute::get('/a', 'A@b');\nRoute::get('/c'", &[]);
    assert_eq!(laravel.routes().len(), 1);
}

#[test]
fn links_name_the_route_or_flag_the_missing_name() {
    let laravel = table(WEB, WEB_IMPORTS);
    let link = |call: &Call, lang| laravel.link_text(call, &[], None, lang);
    assert_eq!(
        link(&js_route("register", 4), Lang::En).as_deref(),
        Some(" → GET /register · RegisteredUserController@create")
    );
    assert_eq!(
        link(&js_route("register.store", 4), Lang::Hu).as_deref(),
        Some(" → POST /register · RegisteredUserController@store")
    );
    assert_eq!(
        link(&js_route("nope", 4), Lang::En).as_deref(),
        Some(" ⚠ no route")
    );
    assert_eq!(
        link(&js_route("nope", 4), Lang::Hu).as_deref(),
        Some(" ⚠ nincs ilyen route")
    );
    // dinamikus argumentum (nincs `route::` név), nem route-hívás, és jelölő nélküli `route()`: nincs link
    let dynamic = call(
        "func_call",
        "route",
        Some("ziggy::route"),
        "route_helper",
        4,
    );
    assert_eq!(link(&dynamic, Lang::En), None);
    let other = call("func_call", "t", Some("i18n::x"), "i18n_helper", 4);
    assert_eq!(link(&other, Lang::En), None);
    // üres route-tábla: a hiányzó név nem állítható
    let empty = table("<?php\n", &[]);
    assert_eq!(
        empty.link_text(&js_route("x", 1), &[], None, Lang::En),
        None
    );
    assert!(empty.lookup(&js_route("x", 1)).is_none());
}

#[test]
fn page_routes_link_to_the_page_file_when_inertia_knows_it() {
    let laravel = table(
        "<?php\nRoute::inertia('/', 'Dashboard')->name('dashboard');\n",
        &[],
    );
    let inertia = Inertia::fixture(
        &root(),
        "resources/js/Pages",
        &["resources/js/Pages/Dashboard.vue"],
        &[],
    );
    let call = js_route("dashboard", 2);
    assert_eq!(
        laravel
            .link_text(&call, &[], Some(&inertia), Lang::En)
            .as_deref(),
        Some(" → GET / · Pages/Dashboard.vue")
    );
    assert_eq!(
        laravel.link_text(&call, &[], None, Lang::En).as_deref(),
        Some(" → GET / · page Dashboard")
    );
}

/// `form.post(route('register'))`: a nevesített `GET /register` helyett az azonos URI-jú `POST` route.
#[test]
fn submit_on_the_same_line_picks_the_matching_verb() {
    let laravel = table(WEB, WEB_IMPORTS);
    let route = js_route("register", 4);
    let post = call(
        "method_call",
        "form.post",
        Some("@inertiajs/vue3::useForm().post"),
        "inertia",
        4,
    );
    let siblings = vec![post.clone(), route.clone()];
    assert_eq!(
        laravel
            .link_text(&siblings[1], &siblings, None, Lang::En)
            .as_deref(),
        Some(" → POST /register · RegisteredUserController@store")
    );
    // más sorban álló küldés nem számít
    let mut elsewhere = post;
    elsewhere.line = 9;
    let siblings = vec![elsewhere, route];
    assert_eq!(
        laravel
            .link_text(&siblings[1], &siblings, None, Lang::En)
            .as_deref(),
        Some(" → GET /register · RegisteredUserController@create")
    );
}

#[test]
fn controller_methods_list_their_routes_up_to_three() {
    let laravel = table(WEB, WEB_IMPORTS);
    let namespace = Some("App\\Http\\Controllers\\Auth");
    assert_eq!(
        laravel
            .route_line(namespace, "RegisteredUserController", "store")
            .as_deref(),
        Some("route: POST /register (register.store)")
    );
    assert_eq!(
        laravel.route_line(namespace, "RegisteredUserController", "destroy"),
        None
    );
    // más névtér: nem ugyanaz az osztály
    assert_eq!(
        laravel.route_line(Some("App\\Other"), "RegisteredUserController", "store"),
        None
    );
    let many = table(
        "<?php\nRoute::get('a', [X::class, 'm']);\nRoute::get('b', [X::class, 'm'])->name('b');\nRoute::get('c', [X::class, 'm']);\nRoute::get('d', [X::class, 'm']);\nRoute::get('e', [X::class, 'm']);\n",
        &[],
    );
    assert_eq!(
        many.route_line(None, "X", "m").as_deref(),
        Some("route: GET /a, GET /b (b), GET /c, +2")
    );
}

#[test]
fn the_ziggy_view_flags_unknown_names_and_counts_the_rest() {
    let web = "<?php\nRoute::get('/', [HomeController::class, 'index'])->name('home');\nRoute::get('/about', [HomeController::class, 'about'])->name('about');\n";
    let controller = "<?php\n\nreturn to_route('nope');\n";
    let files = MemFiles::new(&[
        ("routes/web.php", web),
        ("app/Http/Controllers/X.php", controller),
    ]);
    let routes = graph("/p/routes/web.php", None, &[], Vec::new());
    let vue = graph(
        "/p/resources/js/Pages/Foo.vue",
        Some("resources/js/Pages/Foo.vue"),
        &[],
        vec![
            js_route("home", 3),
            js_route("ghost", 12),
            call(
                "func_call",
                "route",
                Some("ziggy::route"),
                "route_helper",
                14,
            ),
            call("func_call", "route", None, "other", 15),
        ],
    );
    let php = graph(
        "/p/app/Http/Controllers/X.php",
        None,
        &[],
        vec![
            call("func_call", "to_route", None, "framework_use", 3),
            // a `$request->route('param')` nem route-hivatkozás
            {
                let mut request = call("method_call", "route", None, "external_call", 9);
                request.on = Some("$request".into());
                request
            },
        ],
    );
    let (web_path, vue_path, php_path) = (
        root().join("routes/web.php"),
        root().join("resources/js/Pages/Foo.vue"),
        root().join("app/Http/Controllers/X.php"),
    );
    let mut laravel = Laravel::new(&root());
    laravel.scan(
        [
            (web_path.as_path(), &routes),
            (vue_path.as_path(), &vue),
            (php_path.as_path(), &php),
        ],
        &files,
        true,
    );
    let scope = vec![
        "resources/js/Pages/Foo.vue".to_owned(),
        "app/Http/Controllers/X.php".to_owned(),
    ];
    assert_eq!(
        laravel.view_lines(&scope, Lang::En),
        vec![
            "⚠ unknown route name 'nope'  app/Http/Controllers/X.php L3".to_owned(),
            "⚠ unknown route name 'ghost'  resources/js/Pages/Foo.vue L12".to_owned(),
            "routes: 2 named, 1 referenced from JS".to_owned(),
        ]
    );
    // a hatókörön kívüli fájl hibája nem jelenik meg; az összegzés a projekté
    assert_eq!(
        laravel.view_lines(&scope[..1], Lang::Hu),
        vec![
            "⚠ ismeretlen route-név 'ghost'  resources/js/Pages/Foo.vue L12".to_owned(),
            "route-ok: 2 elnevezett, 1 hivatkozva JS-ből".to_owned(),
        ]
    );
    // route-tábla nélkül nincs sor
    assert!(
        Laravel::new(&root())
            .view_lines(&scope, Lang::En)
            .is_empty()
    );
}

#[test]
fn the_strategy_needs_the_laravel_package() {
    let package = |name: &str| Package {
        name: name.into(),
        ecosystem: Ecosystem::Php,
        declared: Some("*".into()),
        installed: None,
        dev: false,
        namespaces: Vec::new(),
        auto_discovered: false,
        plugin: false,
    };
    let packages = |names: &[&str]| {
        Packages::from_map(ProjectPackages {
            root: root(),
            parts: vec![EcosystemPackages {
                root: root(),
                ecosystem: Ecosystem::Php,
                name: None,
                packages: names.iter().map(|n| package(n)).collect(),
                installed_only: Vec::new(),
                own_namespaces: Vec::new(),
                lock: None,
            }],
        })
    };
    assert!(Laravel::detect(&root(), &packages(&["guzzlehttp/guzzle"])).is_none());
    let with_laravel = packages(&["laravel/framework"]);
    assert!(Laravel::detect(&root(), &with_laravel).is_some());
    assert!(with_laravel.frameworks().contains(&NAME));
}
