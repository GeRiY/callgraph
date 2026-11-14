//! A célzott betöltés (`frameworks::targeted`) tesztjei memóriabeli fájlrendszeren: az egy-fájlos outline-hoz
//! gyűjtött adatok ugyanazokat a sorokat adják, mint a teljes projekt-keresés (route-link, controller `route:` /
//! `form:` sor, Vue `form:` sor, modell `table:` / `fillable:` sor, `rendered by:`), és csak a szükséges fájlokat
//! elemzik.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::super::inertia::Inertia;
use super::super::targeted::{self, Need};
use super::Laravel;
use super::tests::{MemFiles, call, graph, js_route, root};
use crate::i18n::Lang;
use crate::model::{Call, Container, FileGraph};
use crate::query::QueryResult;

pub(super) const USE_FORM: &str = "@inertiajs/vue3::useForm";
pub(super) const CONTROLLERS: &str = "App\\Http\\Controllers";
pub(super) const VUE: &str = "resources/js/Pages/Auth/Register.vue";
pub(super) const OTHER_VUE: &str = "resources/js/Pages/Other.vue";
pub(super) const CONTROLLER: &str = "app/Http/Controllers/RegisterController.php";

pub(super) const ROUTES: &str = "<?php
use App\\Http\\Controllers\\RegisterController;
Route::get('register', [RegisterController::class, 'create'])->name('register');
Route::post('register', [RegisterController::class, 'store']);
";

pub(super) const VUE_TEXT: &str = r"<script setup>
import { useForm } from '@inertiajs/vue3';

const form = useForm({
    name: '',
    email: '',
    legacy: '',
});

const submit = () => {
    form.post(route('register'));
};
</script>
";

pub(super) const CONTROLLER_TEXT: &str = r"<?php
namespace App\Http\Controllers;

use App\Http\Requests\RegisterRequest;

class RegisterController extends Controller
{
    public function create() {}

    public function store(RegisterRequest $request)
    {
        return redirect('/');
    }
}
";

pub(super) const REQUEST_TEXT: &str = r"<?php
namespace App\Http\Requests;

class RegisterRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'name' => 'required|string',
            'email' => ['required', 'email'],
            'terms' => 'required',
        ];
    }
}
";

pub(super) const COMPOSER: &str = r#"{"autoload": {"psr-4": {"App\\": "app/"}}}"#;

fn line_of(text: &str, needle: &str) -> u32 {
    let index = text
        .lines()
        .position(|line| line.contains(needle))
        .expect("needle in text");
    u32::try_from(index).unwrap() + 1
}

/// A Vue-fájl hívásai: `useForm`, a küldés és a `route('register')` (ugyanazon a soron).
pub(super) fn vue_calls() -> Vec<Call> {
    let post_line = line_of(VUE_TEXT, "form.post");
    let mut form = call(
        "func_call",
        "useForm",
        Some(USE_FORM),
        "inertia",
        line_of(VUE_TEXT, "const form"),
    );
    form.caller = Some("__top__".into());
    let mut post = call(
        "method_call",
        "form.post",
        Some(&format!("{USE_FORM}().post")),
        "inertia",
        post_line,
    );
    post.caller = Some("submit".into());
    post.container = Some("Page".into());
    vec![form, post, js_route("register", post_line)]
}

pub(super) fn vue_graph() -> FileGraph {
    graph(&format!("/p/{VUE}"), Some(VUE), &[], vue_calls())
}

pub(super) fn controller_graph() -> FileGraph {
    let mut g = graph(&format!("/p/{CONTROLLER}"), None, &[], Vec::new());
    g.namespace = Some(CONTROLLERS.into());
    g.containers.push(Container {
        kind: "class".into(),
        name: "RegisterController".into(),
        line: 6,
        end_line: 14,
        is_abstract: None,
        is_final: None,
        extends: Some(vec!["Controller".into()]),
        implements: None,
        traits: None,
        doc: None,
    });
    g
}

/// A projekt fájljai (a `useForm`-ot nem tartalmazó `Other.vue` és a nem route-fájl is ott van).
fn files() -> MemFiles {
    MemFiles::new(&[
        ("composer.json", COMPOSER),
        ("routes/web.php", ROUTES),
        (CONTROLLER, CONTROLLER_TEXT),
        ("app/Http/Requests/RegisterRequest.php", REQUEST_TEXT),
        (VUE, VUE_TEXT),
        (OTHER_VUE, "<script setup>\nconst a = 1;\n</script>\n"),
    ])
}

/// A fájl gráfja az elemzőtől (a route-fájl `use`-aival).
pub(super) fn analyzed() -> HashMap<PathBuf, FileGraph> {
    let imports = [(
        "RegisterController",
        "App\\Http\\Controllers\\RegisterController",
    )];
    HashMap::from([
        (
            root().join("routes/web.php"),
            graph("/p/routes/web.php", None, &imports, Vec::new()),
        ),
        (root().join(VUE), vue_graph()),
        (
            root().join(OTHER_VUE),
            graph("/p/o.vue", Some("o"), &[], Vec::new()),
        ),
    ])
}

/// A teljes keresés: minden fájl gráfja.
fn full(mem: &MemFiles) -> Laravel {
    let map = analyzed();
    let graphs: Vec<(&Path, &FileGraph)> = map
        .iter()
        .map(|(path, graph)| (path.as_path(), graph))
        .collect();
    let mut laravel = Laravel::new(&root());
    laravel.scan(graphs.iter().copied(), mem, false);
    laravel.scan_forms(graphs.iter().copied(), mem);
    laravel
}

/// A célzott keresés a sorokra; a második: az elemzett fájlok.
fn narrow_scan(mem: &MemFiles, rows: &[QueryResult], need: Need) -> (Laravel, Vec<PathBuf>) {
    let map = analyzed();
    let seen = RefCell::new(Vec::new());
    let analyze = |path: &Path| {
        seen.borrow_mut().push(path.to_path_buf());
        map.get(path).cloned()
    };
    let mut laravel = Laravel::new(&root());
    targeted::scan(rows, None, Some(&mut laravel), need, &root(), mem, &analyze);
    let mut seen = seen.into_inner();
    seen.sort();
    (laravel, seen)
}

const ROUTES_AND_FORMS: Need = Need {
    inertia: false,
    routes: true,
    forms: true,
    models: false,
};

#[test]
fn a_vue_page_gets_the_same_route_link_and_form_line_from_its_own_row() {
    let mem = files();
    let full = full(&mem);
    let rows = [QueryResult::new(vue_graph(), None)];
    let (targeted, seen) = narrow_scan(&mem, &rows, ROUTES_AND_FORMS);
    let calls = vue_calls();
    let link = |laravel: &Laravel| laravel.link_text(&calls[2], &calls, None, Lang::En);
    assert!(link(&full).is_some_and(|text| text.contains("POST /register")));
    assert_eq!(link(&targeted), link(&full));
    let form = |laravel: &Laravel| {
        laravel.form_lines(&format!("/p/{VUE}"), Some("Page"), "submit", Lang::En)
    };
    assert_eq!(form(&full).len(), 1);
    assert!(form(&full)[0].contains("unvalidated: legacy"));
    assert_eq!(form(&targeted), form(&full));
    // semmit nem elemzett: a route-fájlnak csak az importjai kellenek (gyors olvasó), a controller és a FormRequest
    // szövege a PSR-4-ből jött
    assert!(seen.is_empty(), "{seen:?}");
}

#[test]
fn a_controller_gets_the_same_route_and_form_lines_from_the_use_form_files_only() {
    let mem = files();
    let full = full(&mem);
    let rows = [QueryResult::new(controller_graph(), None)];
    let (targeted, seen) = narrow_scan(&mem, &rows, ROUTES_AND_FORMS);
    let route = |laravel: &Laravel, method: &str| {
        laravel.route_line(Some(CONTROLLERS), "RegisterController", method)
    };
    assert_eq!(
        route(&full, "store").as_deref(),
        Some("route: POST /register")
    );
    assert_eq!(route(&targeted, "store"), route(&full, "store"));
    assert_eq!(route(&targeted, "create"), route(&full, "create"));
    let form =
        |laravel: &Laravel| laravel.form_line(Some(CONTROLLERS), "RegisterController", "store");
    assert_eq!(
        form(&full).as_deref(),
        Some("form: Pages/Auth/Register.vue")
    );
    assert_eq!(form(&targeted), form(&full));
    // csak a `useForm`-ot tartalmazó oldal van elemezve (az `Other.vue` nem; a route-fájl importjai gyors olvasóval)
    assert_eq!(seen, [root().join(VUE)]);
}

pub(super) const MODEL_TEXT: &str = "<?php

namespace App\\Models;

class Post extends Model
{
    protected $fillable = ['title', 'nick'];

    protected $casts = ['published' => 'boolean'];
}
";

pub(super) const MIGRATION: &str = "<?php
use Illuminate\\Database\\Migrations\\Migration;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('posts', function (Blueprint $table) {
            $table->id();
            $table->string('title');
            $table->boolean('published');
            $table->timestamps();
        });
    }

    public function down(): void
    {
    }
};
";

pub(super) fn model_graph(extends: &str) -> FileGraph {
    let mut g = graph("/p/app/Models/Post.php", None, &[], Vec::new());
    g.namespace = Some("App\\Models".into());
    g.containers.push(Container {
        kind: "class".into(),
        name: "Post".into(),
        line: 5,
        end_line: 10,
        is_abstract: None,
        is_final: None,
        extends: Some(vec![extends.into()]),
        implements: None,
        traits: None,
        doc: None,
    });
    g
}

#[test]
fn a_model_gets_the_same_table_and_fillable_lines_without_the_project_graph() {
    let mem = MemFiles::new(&[
        ("app/Models/Post.php", MODEL_TEXT),
        ("database/migrations/2024_01_01_000000_posts.php", MIGRATION),
    ]);
    let graph = model_graph("Model");
    let path = root().join("app/Models/Post.php");
    let mut full = Laravel::new(&root());
    full.scan_models([(path.as_path(), &graph)], &mem);
    let rows = [QueryResult::new(graph.clone(), None)];
    let need = Need {
        models: true,
        ..Need::default()
    };
    let (targeted, seen) = narrow_scan(&mem, &rows, need);
    let lines = |laravel: &Laravel| laravel.model_lines(Some("App\\Models"), "Post", Lang::En);
    let expected = lines(&full);
    assert!(expected[0].starts_with("table: posts · id, title"));
    assert!(
        expected
            .iter()
            .any(|line| line.starts_with("fillable: title, nick"))
    );
    assert_eq!(lines(&targeted), expected);
    assert_eq!(
        targeted.model_json(Some("App\\Models"), "Post"),
        full.model_json(Some("App\\Models"), "Post")
    );
    assert!(seen.is_empty());
}

#[test]
fn only_a_non_base_parent_in_a_model_file_asks_for_the_project_graph() {
    let direct = QueryResult::new(model_graph("Model"), None);
    let indirect = QueryResult::new(model_graph("BaseModel"), None);
    assert!(!targeted::maybe_indirect_model(&direct));
    assert!(targeted::maybe_indirect_model(&indirect));
    // a controller nem modell-fájl: nem kér teljes keresést
    assert!(!targeted::maybe_indirect_model(&QueryResult::new(
        controller_graph(),
        None
    )));
}

#[test]
fn the_rendered_by_line_comes_from_the_php_files_that_mention_inertia_only() {
    let controller = "<?php
namespace App\\Http\\Controllers;

class BoardController
{
    public function show()
    {
        return Inertia::render('Board', ['a' => 1]);
    }
}
";
    let mem = MemFiles::new(&[
        ("app/Http/Controllers/BoardController.php", controller),
        ("app/Http/Controllers/Plain.php", "<?php\nclass Plain {}\n"),
        ("resources/js/Pages/Board.vue", "<template></template>\n"),
    ]);
    let mut render = call(
        "static_call",
        "Inertia::render",
        Some("Inertia\\Inertia::render"),
        "framework_use",
        line_of(controller, "Inertia::render"),
    );
    render.container = Some("BoardController".into());
    render.caller = Some("show".into());
    let board = graph(
        "/p/app/Http/Controllers/BoardController.php",
        None,
        &[],
        vec![render],
    );
    let board_path = root().join("app/Http/Controllers/BoardController.php");
    let page = "resources/js/Pages/Board.vue";
    let fresh = || Inertia::fixture(&root(), "resources/js/Pages", &[page], &[]);
    let mut full = fresh();
    full.scan([(board_path.as_path(), &board)], &mem);
    let mut narrow = fresh();
    let seen = RefCell::new(Vec::new());
    let analyze = |path: &Path| {
        seen.borrow_mut().push(path.to_path_buf());
        (path == board_path).then(|| board.clone())
    };
    let need = Need {
        inertia: true,
        ..Need::default()
    };
    targeted::scan(&[], Some(&mut narrow), None, need, &root(), &mem, &analyze);
    let expected = full.rendered_by(&format!("/p/{page}"));
    assert_eq!(expected.as_deref(), Some("BoardController::show L8"));
    assert_eq!(narrow.rendered_by(&format!("/p/{page}")), expected);
    assert_eq!(seen.into_inner(), [board_path]);
}

/// A route-fájlok gyors import-olvasója ugyanazt adja, mint a teljes PHP-elemzés (egyszerű, `as`-os, csoportos).
#[test]
fn imports_only_matches_the_php_analyzer() {
    let text = "<?php\n\nuse App\\Http\\Controllers\\ProfileController;\nuse App\\Http\\Controllers\\Auth\\{LoginController, LogoutController as Out};\nuse Illuminate\\Support\\Facades\\Route as R;\nuse function App\\helper;\n\nR::get('/', [ProfileController::class, 'edit']);\n";
    let fast = targeted::imports_only(Path::new("/p/routes/web.php"), text);
    let full = crate::lang::php::PhpBackend::with_defaults()
        .analyze_source("routes/web.php", text.as_bytes())
        .graph;
    for alias in ["ProfileController", "LoginController", "Out", "R"] {
        assert_eq!(
            fast.imports.get(alias),
            full.imports.get(alias),
            "{alias}: {:?} vs {:?}",
            fast.imports,
            full.imports
        );
    }
}
