//! A Laravel-kötések egységtesztjei memóriabeli fájlrendszeren (`ProjectFiles`): a route-tábla (lánc-név, csoportok,
//! resource-ok, invokálható controller, `Route::inertia`), az outline linkje (létező / hiányzó név, dinamikus
//! argumentum), a controller `route:` sora, a json-hoz a keresés és a `ziggy` nézet sorai.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::super::files::ProjectFiles;
use super::super::inertia::Inertia;
use super::routes::Action;
use super::{Laravel, NAME, route_names};
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
fn arrow_function_groups_chain_dot_prefixed_names() {
    let text = r#"<?php
use App\Http\Controllers\ReportController;

Route::name('a')->group(fn () => Route::prefix('x')->name('.b')->group(fn () =>
    Route::get('/c', [ReportController::class, 'index'])->name('.c')));
Route::name('shop')->prefix('shop')->group(fn () => [
    Route::get('/', [ReportController::class, 'index'])->name('.home'),
    Route::name('.orders')->prefix('orders')->group(function () {
        Route::get('{order}', [ReportController::class, 'show'])->name('.show');
    }),
]);
Route::prefix('x')->name('a')->group(function () {
    Route::prefix('y')->name('.b')->group(function () {
        Route::get('/', [ReportController::class, 'index']);
    });
});
"#;
    let laravel = table(
        text,
        &[(
            "ReportController",
            "App\\Http\\Controllers\\ReportController",
        )],
    );
    assert_eq!(
        rows(&laravel),
        vec![
            row("a.b.c", "GET", "/x/c", "ReportController@index"),
            row("shop.home", "GET", "/shop", "ReportController@index"),
            row(
                "shop.orders.show",
                "GET",
                "/shop/orders/{order}",
                "ReportController@show"
            ),
            // saját `->name()` nélkül: a csoport összefűzött neve (Geri hibajegye)
            row("a.b", "GET", "/x/y", "ReportController@index"),
        ]
    );
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
            // saját név nélküli route a nevesített csoportban: a csoport nevét kapja (Laravel `RouteGroup::merge`)
            row(
                "admin.users.",
                "POST",
                "/admin/users",
                "UserController@store"
            ),
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
fn closure_groups_with_computed_prefix_and_name_keep_a_dynamic_pattern() {
    let text = r#"<?php
foreach (config('app.locales') as $code) {
    Route::prefix($code)->name($code . '.')->group(function () {
        Route::put('/', 'update')->name('update');
    });
}
Route::group(['prefix' => config('x.admin'), 'as' => 'admin.' . $suffix], function () {
    Route::get('users', 'index')->name('users');
});
Route::prefix('admin')->name('admin.')->group(function () {
    Route::get('home', 'index')->name('home');
});
"#;
    let laravel = table(text, &[]);
    let got = rows(&laravel);
    assert_eq!(got[0].0, "{$?}.update", "{got:?}");
    assert_eq!(got[0].2, "/{$?}", "{got:?}");
    assert_eq!(got[1].0, "admin.{$?}users", "{got:?}");
    assert_eq!(got[1].2, "/{$?}/users", "{got:?}");
    assert_eq!(got[2].0, "admin.home", "{got:?}");
    assert_eq!(got[2].2, "/admin/home", "{got:?}");
    assert!(super::matches_dynamic(&got[0].0, "en.update"));
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

/// Csak a betöltött route-fájlok számítanak: a `withRouting` belépési pontjai és a tranzitív `require`-ok; a
/// sehonnan be nem töltött fájl route-jai kimaradnak, és a nézet megnevezi a fájlt.
#[test]
fn only_loaded_route_files_count() {
    let bootstrap = "<?php\nreturn Application::configure()->withRouting(web: __DIR__.'/../routes/web.php', commands: __DIR__.'/../routes/console.php');\n";
    let web = "<?php\nRoute::get('/', fn () => 1)->name('home');\nrequire __DIR__.'/auth.php';\n";
    let auth = "<?php\nRoute::get('/login', fn () => 1)->name('login');\n";
    let dead = "<?php\nRoute::get('/debug', fn () => 1)->name('debug');\n";
    let files = MemFiles::new(&[
        ("bootstrap/app.php", bootstrap),
        ("routes/web.php", web),
        ("routes/auth.php", auth),
        ("routes/test_debug.php", dead),
    ]);
    let graphs: Vec<(PathBuf, FileGraph)> = ["web", "auth", "test_debug"]
        .iter()
        .map(|name| {
            let rel = format!("routes/{name}.php");
            (
                root().join(&rel),
                graph(&format!("/p/{rel}"), None, &[], Vec::new()),
            )
        })
        .collect();
    let mut laravel = Laravel::new(&root());
    laravel.scan(
        graphs.iter().map(|(path, graph)| (path.as_path(), graph)),
        &files,
        false,
    );
    let names: Vec<_> = laravel
        .routes()
        .iter()
        .filter_map(|r| r.name.clone())
        .collect();
    assert_eq!(names, vec!["login".to_owned(), "home".to_owned()]);
    assert_eq!(
        laravel.view_lines(&[], Lang::En),
        vec![
            "? routes/test_debug.php: never loaded (no withRouting / require) — its routes are ignored"
                .to_owned(),
            "routes: 2 named, 0 referenced from JS".to_owned(),
        ]
    );
}

/// Egy csoport által betöltött route-fájl (`->group(base_path(..))`, `->group(__DIR__.'/x.php')`, `require` egy
/// csoportban, a `bootstrap/app.php` `then:` closure-je) a csoport uri- és név-előtagját örökli; a fájl a legfelső
/// szinten nem számít (különben hamis „ismeretlen route-név” jönne az összefűzött nevekre).
#[test]
fn route_files_loaded_from_a_group_inherit_its_prefixes() {
    let bootstrap = "<?php\nreturn Application::configure()->withRouting(web: __DIR__.'/../routes/web.php', then: function () {\n    Route::middleware('web')->prefix('admin')->name('admin')->group(base_path('routes/admin.php'));\n});\n";
    let web = "<?php\nRoute::get('/', fn () => 1)->name('home');\nRoute::name('shop.')->prefix('shop')->group(__DIR__.'/shop.php');\nRoute::name('acct.')->group(function () {\n    require __DIR__.'/account.php';\n});\n";
    let admin = "<?php\nRoute::name('.reports')->prefix('reports')->group(fn () =>\n    Route::get('{r}', fn () => 1)->name('.show'));\n";
    let shop = "<?php\nRoute::get('/', fn () => 1)->name('index');\n";
    let account = "<?php\nRoute::get('/me', fn () => 1)->name('me');\n";
    let files = MemFiles::new(&[
        ("bootstrap/app.php", bootstrap),
        ("routes/web.php", web),
        ("routes/admin.php", admin),
        ("routes/shop.php", shop),
        ("routes/account.php", account),
    ]);
    let graphs: Vec<(PathBuf, FileGraph)> = ["account", "admin", "shop", "web"]
        .iter()
        .map(|name| {
            let rel = format!("routes/{name}.php");
            (
                root().join(&rel),
                graph(&format!("/p/{rel}"), None, &[], Vec::new()),
            )
        })
        .collect();
    let mut laravel = Laravel::new(&root());
    laravel.scan(
        graphs.iter().map(|(path, graph)| (path.as_path(), graph)),
        &files,
        false,
    );
    let mut found: Vec<(String, String)> = laravel
        .routes()
        .iter()
        .map(|r| (r.name.clone().unwrap_or_default(), r.uri.clone()))
        .collect();
    found.sort();
    let pair = |n: &str, u: &str| (n.to_owned(), u.to_owned());
    assert_eq!(
        found,
        vec![
            pair("acct.me", "/me"),
            pair("admin.reports.show", "/admin/reports/{r}"),
            pair("home", "/"),
            pair("shop.index", "/shop"),
        ]
    );
    // a `rename route` névlistája is ugyanígy fűz
    let mut names = route_names(&[
        ("routes/web.php".to_owned(), web.to_owned()),
        ("routes/shop.php".to_owned(), shop.to_owned()),
        ("routes/account.php".to_owned(), account.to_owned()),
    ]);
    names.sort();
    assert_eq!(names, vec!["acct.me", "home", "shop.index"]);
}

/// A feltételesen `require`-olt, a bejárásból kimaradt (pl. `.gitignore`-olt, csak helyben létező) route-fájl
/// route-jai is számítanak: a lemezről olvassuk.
#[test]
fn a_conditionally_required_file_outside_the_scan_still_counts() {
    let bootstrap =
        "<?php\nreturn Application::configure()->withRouting(web: __DIR__.'/../routes/web.php');\n";
    let web = "<?php\nRoute::get('/', fn () => 1)->name('home');\nif (app()->isLocal() && file_exists(__DIR__.'/local.php')) require __DIR__.'/local.php';\n";
    let local = "<?php\nRoute::get('/debug', fn () => 1)->name('debug');\n";
    let files = MemFiles::new(&[
        ("bootstrap/app.php", bootstrap),
        ("routes/web.php", web),
        ("routes/local.php", local),
    ]);
    // a gráfok között csak a web.php van (a local.php a bejárásból kimaradt)
    let web_graph = graph("/p/routes/web.php", None, &[], Vec::new());
    let path = root().join("routes/web.php");
    let mut laravel = Laravel::new(&root());
    laravel.scan([(path.as_path(), &web_graph)], &files, false);
    let mut names: Vec<_> = laravel
        .routes()
        .iter()
        .filter_map(|r| r.name.clone())
        .collect();
    names.sort();
    assert_eq!(names, vec!["debug".to_owned(), "home".to_owned()]);
}

/// A `routes/api.php` route-jai `/api` előtagot kapnak (vagy a `bootstrap/app.php` `apiPrefix:`-ét).
#[test]
fn api_routes_get_the_api_prefix() {
    let api = "<?php\nRoute::apiResource('/location', LocationController::class)->only(['index']);\nRoute::get('/', fn () => 1)->name('root');\n";
    let routes = graph("/p/routes/api.php", None, &[], Vec::new());
    let api_path = root().join("routes/api.php");
    let uris = |files: &MemFiles| {
        let mut laravel = Laravel::new(&root());
        laravel.scan([(api_path.as_path(), &routes)], files, false);
        laravel
            .routes()
            .iter()
            .map(|r| r.uri.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        uris(&MemFiles::new(&[("routes/api.php", api)])),
        vec!["/api/location".to_owned(), "/api".to_owned()]
    );
    let custom = "<?php\nreturn Application::configure()->withRouting(api: __DIR__.'/../routes/api.php', apiPrefix: 'api/v2');\n";
    assert_eq!(
        uris(&MemFiles::new(&[
            ("routes/api.php", api),
            ("bootstrap/app.php", custom)
        ]))[0],
        "/api/v2/location"
    );
}

/// Téves riasztások ellen: a `$notifiable->route('mail', ..)` nem route-hivatkozás, a `redirect()->route('x')` igen;
/// a `Route::has('x')`-szel őrzött hivatkozás nem hiba; dinamikus nevű route-ok mellett csak `?` gyanú.
#[test]
fn notifiable_routes_guards_and_dynamic_names_are_not_false_alarms() {
    let web = "<?php\nRoute::get('/', [HomeController::class, 'index'])->name('home');\n";
    let notification = "<?php\n$n = (new AnonymousNotifiable)->route('mail', 'a@b.c');\nreturn redirect()->route('gone');\nif (Route::has('register')) {\n    return route('register');\n}\n";
    let files = MemFiles::new(&[
        ("routes/web.php", web),
        ("app/Notifications/N.php", notification),
    ]);
    let routes = graph("/p/routes/web.php", None, &[], Vec::new());
    let expr = |line| {
        let mut c = call("method_call", "route", None, "external_call", line);
        c.on = Some("<expr>".into());
        c
    };
    let php = graph(
        "/p/app/Notifications/N.php",
        None,
        &[],
        vec![
            expr(2),
            expr(3),
            call("func_call", "route", None, "global_helper", 5),
        ],
    );
    let (web_path, php_path) = (
        root().join("routes/web.php"),
        root().join("app/Notifications/N.php"),
    );
    let mut laravel = Laravel::new(&root());
    laravel.scan(
        [(web_path.as_path(), &routes), (php_path.as_path(), &php)],
        &files,
        true,
    );
    let scope = vec!["app/Notifications/N.php".to_owned()];
    assert_eq!(
        laravel.view_lines(&scope, Lang::En),
        vec![
            "⚠ unknown route name 'gone'  app/Notifications/N.php L3".to_owned(),
            "routes: 1 named, 0 referenced from JS".to_owned(),
        ]
    );

    // dinamikus név a route-fájlban: a gyanú `?`, és egy sor megmondja, hol
    let dynamic = "<?php\nRoute::get('/', [HomeController::class, 'index'])->name('home');\nforeach ($pages as $name => $page) {\n    Route::get($page['uri'], $h)->name($name);\n}\n";
    let files = MemFiles::new(&[
        ("routes/web.php", dynamic),
        ("app/Notifications/N.php", notification),
    ]);
    let mut laravel = Laravel::new(&root());
    laravel.scan(
        [(web_path.as_path(), &routes), (php_path.as_path(), &php)],
        &files,
        true,
    );
    assert_eq!(
        laravel.view_lines(&scope, Lang::En),
        vec![
            "? unknown route name 'gone'  app/Notifications/N.php L3".to_owned(),
            "? dynamic route names at routes/web.php L4: the names above may be defined there"
                .to_owned(),
            "routes: 1 named, 0 referenced from JS".to_owned(),
        ]
    );
    assert_eq!(
        laravel
            .link_text(&js_route("ghost", 1), &[], None, Lang::En)
            .as_deref(),
        Some(" ? no static route")
    );
}

/// Ciklusban, interpolált névvel felvett (lokalizált) route-ok: a névre illeszkedő `route('en.home')` nem
/// „ismeretlen”, hanem „dinamikus, nem ellenőrizhető”; a route-fájlon kívüli segédnél a `?` gyanú és a hely.
#[test]
fn looped_interpolated_route_names_are_dynamic_not_unknown() {
    let looped = "<?php\nRoute::get('/', [HomeController::class, 'index'])->name('home');\nforeach (['en', 'de'] as $code) {\n    Route::get(\"/{$code}/x\", $h)->name(\"{$code}.x\");\n}\n";
    let vue = graph(
        "/p/resources/js/Pages/Foo.vue",
        Some("resources/js/Pages/Foo.vue"),
        &[],
        vec![js_route("en.x", 3), js_route("zz", 4)],
    );
    let routes = graph("/p/routes/web.php", None, &[], Vec::new());
    let (web_path, vue_path) = (
        root().join("routes/web.php"),
        root().join("resources/js/Pages/Foo.vue"),
    );
    let scope = vec!["resources/js/Pages/Foo.vue".to_owned()];
    let mut laravel = Laravel::new(&root());
    laravel.scan(
        [(web_path.as_path(), &routes), (vue_path.as_path(), &vue)],
        &MemFiles::new(&[("routes/web.php", looped)]),
        true,
    );
    let lines = laravel.view_lines(&scope, Lang::En);
    assert_eq!(
        lines[0],
        "? dynamic route name 'en.x', not verifiable  resources/js/Pages/Foo.vue L3"
    );
    assert!(
        lines[1].starts_with("? unknown route name 'zz'"),
        "{lines:?}"
    );

    // a segéd a route-fájlon kívül: csak a dinamikus név gyanúja és a helye
    let helper = "<?php\nclass LocalizedRoutes {\n    public static function register(array $names) {\n        foreach ($names as $name) {\n            Route::get('/en/'.$name, $h)->name(\"en.{$name}\");\n        }\n    }\n}\n";
    let helper_graph = graph(
        "/p/app/Support/LocalizedRoutes.php",
        None,
        &[],
        vec![call(
            "static_call",
            "Route::get",
            Some("Illuminate\\Support\\Facades\\Route::get"),
            "framework_use",
            5,
        )],
    );
    let helper_path = root().join("app/Support/LocalizedRoutes.php");
    let mut laravel = Laravel::new(&root());
    laravel.scan(
        [
            (web_path.as_path(), &routes),
            (helper_path.as_path(), &helper_graph),
            (vue_path.as_path(), &vue),
        ],
        &MemFiles::new(&[
            (
                "routes/web.php",
                "<?php\nRoute::get('/', $h)->name('home');\n",
            ),
            ("app/Support/LocalizedRoutes.php", helper),
        ]),
        true,
    );
    let lines = laravel.view_lines(&scope, Lang::En);
    assert_eq!(
        lines[2],
        "? dynamic route names at app/Support/LocalizedRoutes.php L5: the names above may be defined there"
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

#[test]
fn a_controller_group_gives_its_routes_prefix_name_action_and_middleware() {
    let text = r#"<?php
Route::middleware(['auth', 'verified'])->prefix('cards')->name('cards.')->controller(ReaderCardController::class)->group(function () {
    Route::get('/', 'index')->name('index');
    Route::post('/', 'store');

    Route::prefix('{card}')->name('item.')->middleware('can:renew')->group(fn () => [
        Route::get('/edit', 'edit')->name('edit'),
    ]);
});
"#;
    let laravel = table(
        text,
        &[(
            "ReaderCardController",
            "App\\Http\\Controllers\\ReaderCardController",
        )],
    );
    let got: Vec<(String, String, String, String, Vec<String>)> = laravel
        .routes()
        .iter()
        .map(|r| {
            (
                r.name.clone().unwrap_or_default(),
                r.verb.clone(),
                r.uri.clone(),
                r.action.label(),
                r.middleware.clone(),
            )
        })
        .collect();
    let mw = |extra: &[&str]| -> Vec<String> {
        ["auth", "verified"]
            .iter()
            .chain(extra)
            .map(|name| (*name).to_owned())
            .collect()
    };
    let row = |name: &str, verb: &str, uri: &str, action: &str, middleware: Vec<String>| {
        (
            name.to_owned(),
            verb.to_owned(),
            uri.to_owned(),
            action.to_owned(),
            middleware,
        )
    };
    assert_eq!(
        got,
        vec![
            row(
                "cards.index",
                "GET",
                "/cards",
                "ReaderCardController@index",
                mw(&[])
            ),
            row(
                "cards.",
                "POST",
                "/cards",
                "ReaderCardController@store",
                mw(&[])
            ),
            row(
                "cards.item.edit",
                "GET",
                "/cards/{card}/edit",
                "ReaderCardController@edit",
                mw(&["can:renew"])
            ),
        ]
    );
}

#[test]
fn a_config_driven_loop_expands_into_named_routes_with_pages() {
    let routes = "<?php\nforeach (config('catalogue.pages') as $name => $page) {\n    Route::get($page['uri'], $page['action'])->name($name);\n}\n";
    let config = "<?php\nuse App\\Http\\Controllers\\HomeController;\nreturn ['pages' => ['home' => ['uri' => '/', 'component' => 'Home', 'action' => HomeController::class], 'about' => ['uri' => '/about', 'action' => HomeController::class]]];\n";
    let files = MemFiles::new(&[("routes/web.php", routes), ("config/catalogue.php", config)]);
    let web = graph("/p/routes/web.php", None, &[], Vec::new());
    let path = root().join("routes/web.php");
    let mut laravel = Laravel::new(&root());
    laravel.scan([(path.as_path(), &web)], &files, false);
    assert_eq!(
        rows(&laravel),
        vec![
            row("home", "GET", "/", "HomeController@__invoke"),
            row("about", "GET", "/about", "HomeController@__invoke"),
        ]
    );
    assert_eq!(laravel.routes()[0].page.as_deref(), Some("Home"));
    assert_eq!(laravel.page_sites().len(), 1);
    // a kibontott ciklus `->name($name)`-je már nem dinamikus
    assert!(laravel.dynamic_names.is_empty());
}
