//! A preset egységtesztjei: memóriabeli fájlrendszerrel és rögzített órával (mint a generátor tesztjei).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use super::{PresetGenerator, spec};
use crate::generate::fs::{Clock, DirEntry, Env, FileSystem};
use crate::generate::generator::{GenRequest, Generator, Target};
use crate::generate::php::PhpEmitter;

const ROOT: &str = "/p";
const T0: u64 = 1_700_000_000;

#[derive(Default)]
struct MemFs {
    files: RefCell<BTreeMap<PathBuf, String>>,
}

impl MemFs {
    fn with(files: &[(&str, &str)]) -> Self {
        let fs = Self::default();
        for (path, text) in files {
            fs.files
                .borrow_mut()
                .insert(Path::new(ROOT).join(path), (*text).to_owned());
        }
        fs
    }

    fn get(&self, rel: &str) -> Option<String> {
        self.files.borrow().get(&Path::new(ROOT).join(rel)).cloned()
    }

    fn put(&self, rel: &str, text: &str) {
        self.files
            .borrow_mut()
            .insert(Path::new(ROOT).join(rel), text.to_owned());
    }
}

impl FileSystem for MemFs {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), content.to_owned());
        Ok(())
    }

    fn create_new(&self, path: &Path, content: &str) -> io::Result<()> {
        if self.files.borrow().contains_key(path) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        self.write(path, content)
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(path)
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        let mut entries: BTreeMap<PathBuf, bool> = BTreeMap::new();
        for path in self.files.borrow().keys() {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else {
                continue;
            };
            let is_dir = parts.next().is_some();
            entries.insert(dir.join(first.as_os_str()), is_dir);
        }
        if entries.is_empty() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        Ok(entries
            .into_iter()
            .map(|(path, is_dir)| DirEntry { path, is_dir })
            .collect())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.files.borrow_mut().remove(path);
        Ok(())
    }
}

struct FixedClock(Cell<u64>);

impl Clock for FixedClock {
    fn now_secs(&self) -> u64 {
        self.0.get()
    }
}

fn env<'a>(fs: &'a MemFs, clock: &'a FixedClock) -> Env<'a> {
    Env {
        root: Path::new(ROOT),
        fs,
        clock,
    }
}

const COMPOSER: &str = r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#;

const WEB: &str = "<?php

use Illuminate\\Support\\Facades\\Route;

Route::get('/', function () {
    return view('welcome');
});
";

const MODEL: &str = "<?php

namespace App\\Models;

use Illuminate\\Database\\Eloquent\\Model;

class Fee extends Model
{
    protected $fillable = ['name', 'amount', 'paid', 'note'];
}
";

const MIGRATION: &str = "<?php

use Illuminate\\Database\\Migrations\\Migration;
use Illuminate\\Database\\Schema\\Blueprint;
use Illuminate\\Support\\Facades\\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('fees', function (Blueprint $table) {
            $table->id();
            $table->string('name');
            $table->integer('amount');
            $table->boolean('paid')->default(false);
            $table->text('note')->nullable();
            $table->timestamps();
        });
    }
};
";

const INPUT_LABEL: &str = "<script setup>
import { ref } from 'vue';

const x = ref(1);
</script>

<template>
    <label><slot /></label>
</template>
";

/// Egy Laravel + Inertia + Vue projekt Breeze-komponensekkel, modellel és migrációval.
fn project() -> MemFs {
    MemFs::with(&[
        ("composer.json", COMPOSER),
        ("routes/web.php", WEB),
        ("app/Models/Fee.php", MODEL),
        (
            "database/migrations/2024_01_01_000000_create_fees_table.php",
            MIGRATION,
        ),
        ("resources/js/Components/InputLabel.vue", INPUT_LABEL),
        ("resources/js/Components/TextInput.vue", INPUT_LABEL),
        ("resources/js/Components/InputError.vue", INPUT_LABEL),
        ("resources/js/Components/PrimaryButton.vue", INPUT_LABEL),
        ("resources/js/Layouts/AuthenticatedLayout.vue", INPUT_LABEL),
    ])
}

fn request(expr: &str, dry: bool) -> GenRequest {
    GenRequest {
        expr: expr.to_owned(),
        dry,
        target: Target::Preview,
    }
}

fn run(fs: &MemFs, clock: &FixedClock, expr: &str, dry: bool) -> Result<String, String> {
    PresetGenerator::new(env(fs, clock))
        .run(&request(expr, dry))
        .map(|o| o.lines.join("\n"))
        .map_err(|e| e.to_string())
}

fn has_line(text: &str, wanted: &str) -> bool {
    text.lines().any(|l| l.trim() == wanted)
}

const CREATE: &str = "laravel:Fee>create(name:str,amount:int,paid?:bool,note?:text)";

// ---- kifejezés ----

#[test]
fn parses_actions_options_and_shares_fields() {
    let s = spec::parse("laravel:Fee[auth,prefix=admin]>create(name:str,amount:int)+edit+index")
        .unwrap();
    assert!(s.options.auth);
    assert_eq!(s.options.prefix.as_deref(), Some("admin"));
    assert_eq!(s.fields_of(spec::Action::Edit).unwrap().len(), 2);
    assert_eq!(
        s.ops(),
        vec![
            spec::Op::Index,
            spec::Op::Create,
            spec::Op::Store,
            spec::Op::Edit,
            spec::Op::Update
        ]
    );
}

#[test]
fn bad_expressions_are_one_line_errors() {
    for expr in [
        "laravel:fee>create(name:str)",
        "laravel:Fee>create(name:blob)",
        "laravel:Fee>edit",
        "laravel:Fee>fly",
        "laravel:Fee",
        "laravel:Fee[nope]>index",
    ] {
        let err = spec::parse(expr).unwrap_err().to_string();
        assert!(
            !err.is_empty() && !err.trim().contains('\n'),
            "{expr}: {err}"
        );
    }
}

// ---- create egy üres projekten ----

#[test]
fn create_writes_request_controller_routes_and_page() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, CREATE, false).unwrap();

    let request = fs.get("app/Http/Requests/StoreFeeRequest.php").unwrap();
    assert!(request.contains("namespace App\\Http\\Requests;"));
    assert!(request.contains("'name' => ['required', 'string', 'max:255'],"));
    assert!(request.contains("'amount' => ['required', 'integer'],"));
    // a bool sosem kötelező, az opcionális mező nullable
    assert!(request.contains("'paid' => ['boolean'],"));
    assert!(request.contains("'note' => ['nullable', 'string'],"));
    assert!(!request.contains("'paid' => ['required"));

    let controller = fs.get("app/Http/Controllers/FeeController.php").unwrap();
    assert!(controller.contains("namespace App\\Http\\Controllers;"));
    assert!(controller.contains("use App\\Http\\Requests\\StoreFeeRequest;"));
    assert!(controller.contains("class FeeController extends Controller"));
    assert!(controller.contains("return Inertia::render('Fees/Create');"));
    assert!(
        controller.contains("public function store(StoreFeeRequest $request): RedirectResponse")
    );
    assert!(controller.contains("Fee::create($request->validated());"));
    // nincs index route: vissza
    assert!(controller.contains("return back();"));

    let routes = fs.get("routes/web.php").unwrap();
    assert!(routes.contains("use App\\Http\\Controllers\\FeeController;"));
    assert!(has_line(
        &routes,
        "Route::get('/fees/create', [FeeController::class, 'create'])->name('fees.create');"
    ));
    assert!(has_line(
        &routes,
        "Route::post('/fees', [FeeController::class, 'store'])->name('fees.store');"
    ));
    assert!(routes.contains("Route::get('/', function () {"));

    let page = fs.get("resources/js/Pages/Fees/Create.vue").unwrap();
    assert!(page.contains("import AuthenticatedLayout from '@/Layouts/AuthenticatedLayout.vue';"));
    assert!(page.contains("const form = useForm({"));
    assert!(has_line(&page, "name: '',"));
    assert!(has_line(&page, "amount: null,"));
    assert!(has_line(&page, "paid: false,"));
    assert!(has_line(&page, "note: '',"));
    assert!(page.contains("form.post(route('fees.store'));"));
    assert!(page.contains("<InputLabel for=\"name\" value=\"Name\" />"));
    assert!(page.contains("<InputError class=\"mt-2\" :message=\"form.errors.name\" />"));
    assert!(page.contains("<PrimaryButton :disabled=\"form.processing\">Save</PrimaryButton>"));
    // a kötelező mezőn `required`, az opcionálison és a boolon nincs
    let line_of = |id: &str| {
        page.lines()
            .find(|l| l.contains(&format!("id=\"{id}\"")))
            .unwrap()
            .to_owned()
    };
    assert!(line_of("name").contains(" required"));
    assert!(line_of("amount").contains(" required"));
    assert!(!line_of("note").contains("required"));
    assert!(!line_of("paid").contains("required"));

    assert!(
        out.contains("wrote app/Http/Requests/StoreFeeRequest.php ("),
        "{out}"
    );
    assert!(
        out.contains("wrote app/Http/Controllers/FeeController.php ("),
        "{out}"
    );
    assert!(
        out.contains("routes/web.php: +2 routes (fees.create, fees.store)"),
        "{out}"
    );
    assert!(
        out.contains("wrote resources/js/Pages/Fees/Create.vue ("),
        "{out}"
    );
    assert!(out.contains("✓ form ↔ rules (4 fields)"), "{out}");
    // a modell rendben van: nincs figyelmeztetés
    assert!(!out.contains('⚠'), "{out}");
}

#[test]
fn several_actions_link_the_index_route() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        "laravel:Fee>create(name:str,amount:int)+edit+index+destroy",
        false,
    )
    .unwrap();
    let controller = fs.get("app/Http/Controllers/FeeController.php").unwrap();
    assert!(controller.contains("return to_route('fees.index');"));
    assert!(controller.contains("'fees' => Fee::latest()->paginate(20),"));
    assert!(controller.contains("public function destroy(Fee $fee): RedirectResponse"));
    let index = fs.get("resources/js/Pages/Fees/Index.vue").unwrap();
    assert!(index.contains("<tr v-for=\"item in fees.data\" :key=\"item.id\">"));
    assert!(index.contains("<Link :href=\"route('fees.edit', item.id)\""));
    assert!(
        index.contains("router.delete(route('fees.destroy', item.id))")
            || index.contains("router.delete(route('fees.destroy', id))")
    );
    let routes = fs.get("routes/web.php").unwrap();
    // a `create` az `{fee}`-s route-ok előtt
    let create = routes.find("/fees/create").unwrap();
    let edit = routes.find("/fees/{fee}/edit").unwrap();
    assert!(create < edit);
    assert!(out.contains("✓ form ↔ rules"), "{out}");
    assert!(out.contains("route names"), "{out}");
}

#[test]
fn edit_uses_put_and_the_route_parameter() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    run(&fs, &clock, "laravel:Fee>edit(name:str,paid?:bool)", false).unwrap();
    let page = fs.get("resources/js/Pages/Fees/Edit.vue").unwrap();
    assert!(page.contains("form.put(route('fees.update', props.fee.id));"));
    assert!(has_line(&page, "name: props.fee.name,"));
    assert!(page.contains("const props = defineProps({"));
    let routes = fs.get("routes/web.php").unwrap();
    assert!(has_line(
        &routes,
        "Route::get('/fees/{fee}/edit', [FeeController::class, 'edit'])->name('fees.edit');"
    ));
    assert!(has_line(
        &routes,
        "Route::put('/fees/{fee}', [FeeController::class, 'update'])->name('fees.update');"
    ));
    let controller = fs.get("app/Http/Controllers/FeeController.php").unwrap();
    assert!(
        controller.contains(
            "public function update(UpdateFeeRequest $request, Fee $fee): RedirectResponse"
        )
    );
    assert!(controller.contains("$fee->update($request->validated());"));
    assert!(fs.get("app/Http/Requests/UpdateFeeRequest.php").is_some());
}

// ---- meglévő fájlok ----

const CONTROLLER: &str = "<?php

namespace App\\Http\\Controllers;

use App\\Models\\Fee;

class FeeController extends Controller
{
    public function index()
    {
        return Fee::all();
    }
}
";

#[test]
fn existing_controller_gets_the_methods_inserted() {
    let fs = project();
    fs.put("app/Http/Controllers/FeeController.php", CONTROLLER);
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, CREATE, false).unwrap();
    let controller = fs.get("app/Http/Controllers/FeeController.php").unwrap();
    assert!(controller.contains("return Fee::all();"));
    assert!(controller.contains("public function create(): Response"));
    assert!(
        controller.contains("public function store(StoreFeeRequest $request): RedirectResponse")
    );
    assert!(controller.contains("use App\\Http\\Requests\\StoreFeeRequest;"));
    assert!(controller.contains("use Inertia\\Inertia;"));
    assert_eq!(controller.matches("use App\\Models\\Fee;").count(), 1);
    assert!(
        out.contains("inserted into app/Http/Controllers/FeeController.php: create, store"),
        "{out}"
    );
    assert!(
        !out.contains("wrote app/Http/Controllers/FeeController.php"),
        "{out}"
    );
}

#[test]
fn an_existing_method_or_file_is_rejected_and_nothing_is_written() {
    let fs = project();
    fs.put(
        "app/Http/Controllers/FeeController.php",
        &CONTROLLER.replace("index", "create"),
    );
    let clock = FixedClock(Cell::new(T0));
    let err = run(&fs, &clock, CREATE, false).unwrap_err();
    assert!(err.contains("already has: create"), "{err}");
    assert!(fs.get("app/Http/Requests/StoreFeeRequest.php").is_none());
    assert!(fs.get("resources/js/Pages/Fees/Create.vue").is_none());
    assert_eq!(fs.get("routes/web.php").unwrap(), WEB);

    let fs = project();
    fs.put(
        "resources/js/Pages/Fees/Create.vue",
        "<template></template>\n",
    );
    let err = run(&fs, &clock, CREATE, false).unwrap_err();
    assert!(err.contains("already exists"), "{err}");
    assert!(fs.get("app/Http/Controllers/FeeController.php").is_none());
}

#[test]
fn an_existing_route_name_is_skipped_and_reported() {
    let fs = project();
    fs.put(
        "routes/web.php",
        &format!(
            "{WEB}\nRoute::get('/fees/new', [OtherController::class, 'x'])->name('fees.create');\n"
        ),
    );
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, CREATE, false).unwrap();
    let routes = fs.get("routes/web.php").unwrap();
    assert_eq!(routes.matches("->name('fees.create')").count(), 1);
    assert!(routes.contains("->name('fees.store')"));
    assert!(
        out.contains("skipped existing route(s): fees.create"),
        "{out}"
    );
    assert!(out.contains("+1 routes (fees.store)"), "{out}");
}

#[test]
fn the_auth_option_uses_the_existing_group_or_adds_one() {
    let grouped = "<?php

use Illuminate\\Support\\Facades\\Route;

Route::middleware(['auth', 'verified'])->group(function () {
    Route::get('/dashboard', fn () => 1)->name('dashboard');
});

require __DIR__.'/auth.php';
";
    let fs = project();
    fs.put("routes/web.php", grouped);
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, "laravel:Fee[auth]>create(name:str)", false).unwrap();
    let routes = fs.get("routes/web.php").unwrap();
    let route = routes.find("    Route::get('/fees/create'").expect(&routes);
    assert!(route < routes.find("});").unwrap(), "{routes}");
    assert!(routes.contains(
        "    Route::post('/fees', [FeeController::class, 'store'])->name('fees.store');"
    ));
    assert!(out.contains("in auth group"), "{out}");
    assert!(!out.contains("in new auth group"), "{out}");

    let fs = project();
    let out = run(&fs, &clock, "laravel:Fee[auth]>create(name:str)", false).unwrap();
    let routes = fs.get("routes/web.php").unwrap();
    assert!(
        routes.contains("Route::middleware('auth')->group(function () {"),
        "{routes}"
    );
    assert!(routes.contains("\n});\n"), "{routes}");
    assert!(out.contains("in new auth group"), "{out}");
}

#[test]
fn the_prefix_option_prefixes_uris_and_names() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    run(
        &fs,
        &clock,
        "laravel:Fee[prefix=admin]>create(name:str)",
        false,
    )
    .unwrap();
    let routes = fs.get("routes/web.php").unwrap();
    assert!(has_line(
        &routes,
        "Route::get('/admin/fees/create', [FeeController::class, 'create'])->name('admin.fees.create');"
    ));
    let page = fs.get("resources/js/Pages/Fees/Create.vue").unwrap();
    assert!(page.contains("form.post(route('admin.fees.store'));"));
}

#[test]
fn the_rule_style_follows_the_existing_requests() {
    let fs = project();
    fs.put(
        "app/Http/Requests/LoginRequest.php",
        "<?php

namespace App\\Http\\Requests;

use Illuminate\\Foundation\\Http\\FormRequest;

class LoginRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'email' => 'required|email',
            'password' => 'required|string',
        ];
    }
}
",
    );
    let clock = FixedClock(Cell::new(T0));
    run(&fs, &clock, CREATE, false).unwrap();
    let request = fs.get("app/Http/Requests/StoreFeeRequest.php").unwrap();
    assert!(
        request.contains("'name' => 'required|string|max:255',"),
        "{request}"
    );
    assert!(request.contains("'paid' => 'boolean',"), "{request}");
}

// ---- modell ----

#[test]
fn model_warnings_for_a_field_that_is_not_a_column_or_not_fillable() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        "laravel:Fee>create(name:str,ghost?:str)",
        false,
    )
    .unwrap();
    assert!(
        out.contains("⚠ Fee: not a column of `fees`: ghost"),
        "{out}"
    );
    assert!(out.contains("⚠ Fee: not in $fillable: ghost"), "{out}");
    // figyelmeztetés nem hiba: a fájlok megvannak
    assert!(fs.get("app/Http/Requests/StoreFeeRequest.php").is_some());
}

#[test]
fn a_missing_model_is_reported_not_generated() {
    let fs = MemFs::with(&[("composer.json", COMPOSER), ("routes/web.php", WEB)]);
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, "laravel:Fee>create(name:str)", false).unwrap();
    assert!(out.contains("⚠ model Fee not found"), "{out}");
    assert!(fs.get("app/Models/Fee.php").is_none());
    // komponens nélkül sima HTML
    let page = fs.get("resources/js/Pages/Fees/Create.vue").unwrap();
    assert!(page.contains("<label for=\"name\""));
    assert!(page.contains("<button type=\"submit\" :disabled=\"form.processing\""));
    assert!(!page.contains("AuthenticatedLayout"));
}

// ---- tranzakció ----

#[test]
fn one_undo_reverts_every_file_of_the_preset() {
    let fs = project();
    fs.put("app/Http/Controllers/FeeController.php", CONTROLLER);
    let clock = FixedClock(Cell::new(T0));
    run(&fs, &clock, CREATE, false).unwrap();
    assert_ne!(fs.get("routes/web.php").unwrap(), WEB);
    assert!(fs.get("resources/js/Pages/Fees/Create.vue").is_some());

    let out = Generator::new(env(&fs, &clock), PhpEmitter)
        .undo(None)
        .unwrap();
    let text = out.lines.join("\n");
    assert!(text.contains("restored routes/web.php"), "{text}");
    assert!(
        text.contains("removed app/Http/Requests/StoreFeeRequest.php"),
        "{text}"
    );
    assert_eq!(fs.get("routes/web.php").unwrap(), WEB);
    assert_eq!(
        fs.get("app/Http/Controllers/FeeController.php").unwrap(),
        CONTROLLER
    );
    assert!(fs.get("app/Http/Requests/StoreFeeRequest.php").is_none());
    assert!(fs.get("resources/js/Pages/Fees/Create.vue").is_none());
}

#[test]
fn a_later_single_edit_makes_undo_single_file_again() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    run(&fs, &clock, CREATE, false).unwrap();
    clock.0.set(T0 + 60);
    let generator = Generator::new(env(&fs, &clock), PhpEmitter);
    let replace = GenRequest {
        expr: "+x():int{ return 1; }".to_owned(),
        dry: false,
        target: Target::Into {
            file: "app/Http/Controllers/FeeController.php".to_owned(),
            class: None,
            after: None,
        },
    };
    generator.run(&replace).unwrap();
    let out = generator.undo(None).unwrap().lines.join("\n");
    assert!(
        out.starts_with("restored app/Http/Controllers/FeeController.php"),
        "{out}"
    );
    // a preset többi fájlja megmaradt
    assert!(fs.get("app/Http/Requests/StoreFeeRequest.php").is_some());
}

#[test]
fn a_validation_failure_writes_nothing() {
    let fs = project();
    fs.put("routes/web.php", "<?php\n\nRoute::get('/', function () {\n");
    let clock = FixedClock(Cell::new(T0));
    let err = run(&fs, &clock, CREATE, false).unwrap_err();
    assert!(err.contains("syntax error in routes/web.php"), "{err}");
    assert!(fs.get("app/Http/Requests/StoreFeeRequest.php").is_none());
    assert!(fs.get("app/Http/Controllers/FeeController.php").is_none());
    assert!(fs.get("resources/js/Pages/Fees/Create.vue").is_none());
}

#[test]
fn dry_prints_every_file_under_a_header_and_writes_nothing() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let out = run(&fs, &clock, CREATE, true).unwrap();
    for header in [
        "== app/Http/Requests/StoreFeeRequest.php",
        "== app/Http/Controllers/FeeController.php",
        "== routes/web.php",
        "== resources/js/Pages/Fees/Create.vue",
    ] {
        assert!(out.lines().any(|l| l == header), "{header}\n{out}");
    }
    assert!(
        out.contains("Route::post('/fees', [FeeController::class, 'store'])->name('fees.store');")
    );
    assert!(!out.contains("undo:"), "{out}");
    assert!(fs.get("app/Http/Requests/StoreFeeRequest.php").is_none());
    assert!(fs.get("resources/js/Pages/Fees/Create.vue").is_none());
    assert_eq!(fs.get("routes/web.php").unwrap(), WEB);
}

#[test]
fn a_preset_takes_no_file_target() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let req = GenRequest {
        expr: CREATE.to_owned(),
        dry: false,
        target: Target::Write {
            file: "app/X.php".to_owned(),
        },
    };
    let err = PresetGenerator::new(env(&fs, &clock))
        .run(&req)
        .unwrap_err();
    assert!(err.to_string().contains("picks its files itself"));
}
