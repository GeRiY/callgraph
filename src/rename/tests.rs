//! Az átnevezés egységtesztjei: memóriabeli fájlrendszerrel és rögzített órával (mint a generátor tesztjei).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use super::sites::{Plan, Site, finish};
use super::{Kind, RenameRequest, Renamer};
use crate::generate::fs::{Clock, DirEntry, Env, FileSystem};
use crate::generate::{GenResult, Generator, Output, PhpEmitter};

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

    fn snapshot(&self) -> BTreeMap<PathBuf, String> {
        self.files.borrow().clone()
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

fn request(kind: Kind, old: &str, new: &str) -> RenameRequest {
    RenameRequest {
        kind,
        old: old.to_owned(),
        new: new.to_owned(),
        dry: false,
        strings: false,
        keep_table: false,
    }
}

fn run(fs: &MemFs, req: &RenameRequest) -> GenResult<Output> {
    let clock = FixedClock(Cell::new(T0));
    Renamer::new(env(fs, &clock)).run(req)
}

fn undo(fs: &MemFs) {
    let clock = FixedClock(Cell::new(T0 + 100));
    Generator::new(env(fs, &clock), PhpEmitter)
        .undo(None)
        .expect("undo");
}

const COMPOSER: &str = r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#;

// ---------------------------------------------------------------- route

const WEB: &str = "<?php

use App\\Http\\Controllers\\ProfileController;
use Illuminate\\Support\\Facades\\Route;

Route::middleware('auth')->group(function () {
    Route::get('/profile', [ProfileController::class, 'edit'])->name('profile.edit');
    Route::patch('/profile', [ProfileController::class, 'update'])->name('profile.update');
});

Route::prefix('profile')->name('profile.')->group(function () {
    Route::get('/x', [ProfileController::class, 'edit'])->name('show');
});

Route::get('/home', fn () => view('home'))->name('home');
";

const PROFILE_CONTROLLER: &str = "<?php

namespace App\\Http\\Controllers;

class ProfileController
{
    public function update()
    {
        // route('profile.edit') in a comment is not a reference
        $tab = 'x';
        $dynamic = route('profile.' . $tab);
        return redirect()->route('profile.edit')->with('url', route(\"profile.edit\"));
    }
}
";

const NAV_BLADE: &str = "<nav>
    <a href=\"{{ route('profile.edit') }}\">Profile</a>
</nav>
";

const LAYOUT_VUE: &str = "<script setup>
const url = route(\"profile.edit\");
const active = route().current('profile.edit');
</script>

<template>
    <a :href=\"route('profile.edit')\">Profile</a>
</template>
";

const TABS_VUE: &str = "<script setup>
const tab = 'x';
const a = route('profile.' + tab);
const b = route(`profile.${tab}`);
</script>

<template>
    <div />
</template>
";

fn route_project() -> MemFs {
    MemFs::with(&[
        ("composer.json", COMPOSER),
        ("routes/web.php", WEB),
        (
            "app/Http/Controllers/ProfileController.php",
            PROFILE_CONTROLLER,
        ),
        ("resources/views/nav.blade.php", NAV_BLADE),
        ("resources/js/Layouts/Layout.vue", LAYOUT_VUE),
        ("resources/js/Pages/Tabs.vue", TABS_VUE),
    ])
}

#[test]
fn route_leaf_rename_changes_the_definition_and_every_reference() {
    let fs = route_project();
    let out = run(&fs, &request(Kind::Route, "profile.edit", "account.edit")).unwrap();
    let text = out.render();
    assert!(
        text.starts_with("renamed route profile.edit → account.edit: 4 files, 7 sites"),
        "{text}"
    );
    assert!(text.contains("routes/web.php: 1"), "{text}");
    assert!(text.contains("callgraph undo"), "{text}");
    let web = fs.get("routes/web.php").unwrap();
    assert!(web.contains("->name('account.edit')"), "{web}");
    assert!(web.contains("->name('profile.update')"), "{web}");
    let controller = fs
        .get("app/Http/Controllers/ProfileController.php")
        .unwrap();
    assert!(
        controller.contains("redirect()->route('account.edit')"),
        "{controller}"
    );
    assert!(
        controller.contains("route(\"account.edit\")"),
        "{controller}"
    );
    // a megjegyzés és a dinamikus hivatkozás érintetlen
    assert!(
        controller.contains("// route('profile.edit') in a comment"),
        "{controller}"
    );
    assert!(
        controller.contains("route('profile.' . $tab)"),
        "{controller}"
    );
    let blade = fs.get("resources/views/nav.blade.php").unwrap();
    assert_eq!(blade, NAV_BLADE.replace("profile.edit", "account.edit"));
    let layout = fs.get("resources/js/Layouts/Layout.vue").unwrap();
    assert_eq!(layout, LAYOUT_VUE.replace("profile.edit", "account.edit"));
}

/// `*`-os átnevezés resource-route-okra: a `->names('..')` kerül a hívás után (vagy a meglévő cserélődik), az
/// URI marad, a hivatkozások átíródnak.
#[test]
fn wildcard_rename_of_resource_routes_adds_names() {
    let web = "<?php\n\nuse App\\Http\\Controllers\\ProjectController;\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::resource('projects', ProjectController::class);\nRoute::apiResource('tags', TagController::class)->names('tags');\n";
    let page = "<script setup>\nconst go = () => router.get(route('projects.index'));\n</script>\n<template><a :href=\"route('projects.show', 1)\">x</a></template>\n";
    let fs = MemFs::with(&[
        ("composer.json", COMPOSER),
        ("routes/web.php", web),
        ("resources/js/Pages/P.vue", page),
    ]);
    run(&fs, &request(Kind::Route, "projects.*", "jobs.*")).unwrap();
    let routes = fs.get("routes/web.php").unwrap();
    assert!(
        routes.contains("Route::resource('projects', ProjectController::class)->names('jobs');"),
        "{routes}"
    );
    assert_eq!(
        fs.get("resources/js/Pages/P.vue").unwrap(),
        page.replace("projects.", "jobs.")
    );
    // meglévő `->names('tags')` cserélődik; a generált `ziggy.js`-t nem írjuk, csak jelezzük
    fs.put(
        "resources/js/ziggy.js",
        "const Ziggy = {\"routes\":{\"tags.index\":{\"uri\":\"tags\"}}};\n",
    );
    let out = run(&fs, &request(Kind::Route, "tags.*", "labels.*")).unwrap();
    let routes = fs.get("routes/web.php").unwrap();
    assert!(routes.contains("->names('labels');"), "{routes}");
    let text = out.render();
    assert!(
        text.contains("resources/js/ziggy.js is generated") && text.contains("ziggy:generate"),
        "{text}"
    );
    assert!(
        fs.get("resources/js/ziggy.js")
            .unwrap()
            .contains("tags.index")
    );
}

/// Az aláírt URL-ek (`URL::temporarySignedRoute(` több soron át, `URL::signedRoute(`) és a teszt-segédek
/// (`assertRedirectToRoute(`) is hivatkozások.
#[test]
fn signed_routes_and_test_helpers_are_references_too() {
    let fs = route_project();
    let link = "<?php\n\nnamespace App\\Auth;\n\nuse Illuminate\\Support\\Facades\\URL;\n\nclass Link\n{\n    public static function url(): string\n    {\n        return URL::temporarySignedRoute(\n            'profile.edit',\n            now()->addMinutes(5),\n        );\n    }\n\n    public static function plain(): string\n    {\n        return URL::signedRoute('profile.edit');\n    }\n}\n";
    let test = "<?php\n\nnamespace Tests\\Feature;\n\nclass LinkTest\n{\n    public function test_it(): void\n    {\n        $this->get('/x')->assertRedirectToRoute('profile.edit');\n    }\n}\n";
    fs.put("app/Auth/Link.php", link);
    fs.put("tests/Feature/LinkTest.php", test);
    run(&fs, &request(Kind::Route, "profile.edit", "account.edit")).unwrap();
    assert_eq!(
        fs.get("app/Auth/Link.php").unwrap(),
        link.replace("profile.edit", "account.edit")
    );
    assert_eq!(
        fs.get("tests/Feature/LinkTest.php").unwrap(),
        test.replace("profile.edit", "account.edit")
    );
}

#[test]
fn dynamic_route_references_are_listed_as_uncertain_and_not_changed() {
    let fs = route_project();
    let out = run(&fs, &request(Kind::Route, "profile.edit", "account.edit")).unwrap();
    let text = out.render();
    assert!(text.contains("resources/js/Pages/Tabs.vue"), "{text}");
    assert!(text.contains("route('profile.' + tab)"), "{text}");
    assert!(text.contains("route(`profile.${tab}`)"), "{text}");
    assert_eq!(fs.get("resources/js/Pages/Tabs.vue").unwrap(), TABS_VUE);
}

#[test]
fn route_leaf_under_a_group_prefix_changes_only_the_leaf() {
    let fs = route_project();
    run(&fs, &request(Kind::Route, "profile.show", "profile.detail")).unwrap();
    let web = fs.get("routes/web.php").unwrap();
    assert!(web.contains("->name('detail')"), "{web}");
    assert!(web.contains("->name('profile.')"), "{web}");
}

#[test]
fn changing_a_group_prefix_for_one_route_is_refused_with_a_hint() {
    let fs = route_project();
    let before = fs.snapshot();
    let err = run(&fs, &request(Kind::Route, "profile.show", "account.show"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("profile.*"), "{err}");
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn wildcard_rename_moves_the_whole_prefix_and_the_group() {
    let fs = route_project();
    let out = run(&fs, &request(Kind::Route, "profile.*", "account.*")).unwrap();
    let text = out.render();
    assert!(
        text.starts_with("renamed route profile.* → account.*"),
        "{text}"
    );
    let web = fs.get("routes/web.php").unwrap();
    assert!(web.contains("->name('account.edit')"), "{web}");
    assert!(web.contains("->name('account.update')"), "{web}");
    assert!(web.contains("->name('account.')->group"), "{web}");
    assert!(!web.contains("profile."), "{web}");
    let layout = fs.get("resources/js/Layouts/Layout.vue").unwrap();
    assert!(!layout.contains("profile.edit"), "{layout}");
}

#[test]
fn route_collision_and_unknown_names_are_refused() {
    let fs = route_project();
    let before = fs.snapshot();
    let taken = run(&fs, &request(Kind::Route, "profile.edit", "home"))
        .unwrap_err()
        .to_string();
    assert!(taken.contains("home"), "{taken}");
    let unknown = run(&fs, &request(Kind::Route, "nope.index", "other.index"))
        .unwrap_err()
        .to_string();
    assert!(unknown.contains("nope.index"), "{unknown}");
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn dry_run_writes_nothing_and_previews_every_site() {
    let fs = route_project();
    let before = fs.snapshot();
    let mut req = request(Kind::Route, "profile.edit", "account.edit");
    req.dry = true;
    let text = run(&fs, &req).unwrap().render();
    assert!(text.contains("7 sites"), "{text}");
    assert!(
        text.contains("routes/web.php:L7  profile.edit → account.edit"),
        "{text}"
    );
    assert!(
        text.lines()
            .filter(|l| l.contains("profile.edit → account.edit"))
            .count()
            >= 7,
        "{text}"
    );
    assert!(!text.contains("callgraph undo"), "{text}");
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn one_undo_restores_every_file_of_a_route_rename() {
    let fs = route_project();
    let before = fs.snapshot();
    run(&fs, &request(Kind::Route, "profile.edit", "account.edit")).unwrap();
    assert_ne!(fs.snapshot(), before);
    undo(&fs);
    let after: BTreeMap<PathBuf, String> = fs
        .snapshot()
        .into_iter()
        .filter(|(path, _)| !path.to_string_lossy().contains(".callgraph"))
        .collect();
    assert_eq!(after, before);
}

// ---------------------------------------------------------------- class

const FEE: &str = "<?php

namespace App\\Models;

use Illuminate\\Database\\Eloquent\\Model;

/**
 * @property int $id
 */
class Fee extends Model
{
    public static function make(): static
    {
        return new Fee();
    }
}
";

const ORDER: &str = "<?php

namespace App\\Models;

use Illuminate\\Database\\Eloquent\\Model;

class Order extends Model
{
    public function fees()
    {
        return $this->hasMany(Fee::class);
    }
}
";

const BILLING: &str = r#"<?php

namespace App\Services;

use App\Models\Fee;

class Billing
{
    /**
     * @param Fee $fee
     * @return Fee[]
     */
    public function run(Fee $fee): array
    {
        $kind = Fee::class;
        $name = 'App\\Models\\Fee';
        $y = new \App\Models\Fee();
        return [$fee, $y, $kind, $name];
    }
}
"#;

const ALIASED: &str = "<?php

namespace App\\Services;

use App\\Models\\Fee as Legacy;

class Report
{
    public function find()
    {
        return Legacy::find(1);
    }
}
";

fn class_project() -> MemFs {
    MemFs::with(&[
        ("composer.json", COMPOSER),
        ("app/Models/Fee.php", FEE),
        ("app/Models/Order.php", ORDER),
        ("app/Services/Billing.php", BILLING),
        ("app/Services/Report.php", ALIASED),
    ])
}

#[test]
fn class_rename_moves_the_file_and_rewrites_every_reference() {
    let fs = class_project();
    let out = run(
        &fs,
        &request(Kind::Class, "App\\Models\\Fee", "App\\Models\\Charge"),
    )
    .unwrap();
    let text = out.render();
    assert!(
        text.starts_with("renamed class App\\Models\\Fee → App\\Models\\Charge"),
        "{text}"
    );
    assert!(
        text.contains("app/Models/Fee.php → app/Models/Charge.php"),
        "{text}"
    );
    assert!(fs.get("app/Models/Fee.php").is_none());
    let moved = fs.get("app/Models/Charge.php").unwrap();
    assert!(moved.contains("class Charge extends Model"), "{moved}");
    assert!(moved.contains("return new Charge();"), "{moved}");
    assert!(moved.contains("namespace App\\Models;"), "{moved}");
    let order = fs.get("app/Models/Order.php").unwrap();
    assert!(order.contains("hasMany(Charge::class)"), "{order}");
    let billing = fs.get("app/Services/Billing.php").unwrap();
    assert!(billing.contains("use App\\Models\\Charge;"), "{billing}");
    assert!(billing.contains("@param Charge $fee"), "{billing}");
    assert!(billing.contains("@return Charge[]"), "{billing}");
    assert!(billing.contains("run(Charge $fee)"), "{billing}");
    assert!(billing.contains("$kind = Charge::class;"), "{billing}");
    assert!(billing.contains("new \\App\\Models\\Charge()"), "{billing}");
    // a sztring-hivatkozás jelzés, nem változás
    assert!(billing.contains("'App\\\\Models\\\\Fee'"), "{billing}");
    assert!(
        text.contains("⚠ uncertain (not changed): app/Services/Billing.php"),
        "{text}"
    );
}

#[test]
fn class_rename_keeps_aliases() {
    let fs = class_project();
    run(
        &fs,
        &request(Kind::Class, "App\\Models\\Fee", "App\\Models\\Charge"),
    )
    .unwrap();
    let report = fs.get("app/Services/Report.php").unwrap();
    assert!(
        report.contains("use App\\Models\\Charge as Legacy;"),
        "{report}"
    );
    assert!(report.contains("Legacy::find(1)"), "{report}");
}

#[test]
fn class_rename_accepts_a_unique_short_name_and_warns_about_the_table() {
    let fs = class_project();
    let out = run(&fs, &request(Kind::Class, "Fee", "Charge")).unwrap();
    let text = out.render();
    assert!(text.contains("table name changes fees → charges"), "{text}");
    assert!(fs.get("app/Models/Charge.php").is_some());
}

#[test]
fn keep_table_inserts_the_old_table_name() {
    let fs = class_project();
    let mut req = request(Kind::Class, "Fee", "Charge");
    req.keep_table = true;
    let text = run(&fs, &req).unwrap().render();
    assert!(!text.contains("table name changes"), "{text}");
    let moved = fs.get("app/Models/Charge.php").unwrap();
    assert!(moved.contains("protected $table = 'fees';"), "{moved}");
    assert!(
        moved.contains("class Charge extends Model\n{\n    protected $table = 'fees';\n"),
        "{moved}"
    );
}

#[test]
fn strings_flag_rewrites_class_name_literals() {
    let fs = class_project();
    let mut req = request(Kind::Class, "App\\Models\\Fee", "App\\Models\\Charge");
    req.strings = true;
    run(&fs, &req).unwrap();
    let billing = fs.get("app/Services/Billing.php").unwrap();
    assert!(billing.contains("'App\\\\Models\\\\Charge'"), "{billing}");
}

#[test]
fn class_namespace_change_moves_the_file_and_adds_the_missing_import() {
    let fs = class_project();
    run(
        &fs,
        &request(Kind::Class, "App\\Models\\Fee", "App\\Billing\\Charge"),
    )
    .unwrap();
    let moved = fs.get("app/Billing/Charge.php").unwrap();
    assert!(moved.contains("namespace App\\Billing;"), "{moved}");
    assert!(moved.contains("class Charge extends Model"), "{moved}");
    let order = fs.get("app/Models/Order.php").unwrap();
    assert!(order.contains("use App\\Billing\\Charge;"), "{order}");
    assert!(order.contains("hasMany(Charge::class)"), "{order}");
    let billing = fs.get("app/Services/Billing.php").unwrap();
    assert!(billing.contains("use App\\Billing\\Charge;"), "{billing}");
    assert!(
        billing.contains("new \\App\\Billing\\Charge()"),
        "{billing}"
    );
}

#[test]
fn class_rename_refuses_existing_targets_and_ambiguous_short_names() {
    let fs = class_project();
    fs.write(
        Path::new(ROOT).join("app/Other/Order.php").as_path(),
        "<?php\n\nnamespace App\\Other;\n\nclass Order\n{\n}\n",
    )
    .unwrap();
    let before = fs.snapshot();
    let taken = run(
        &fs,
        &request(Kind::Class, "App\\Models\\Fee", "App\\Models\\Order"),
    )
    .unwrap_err()
    .to_string();
    assert!(taken.contains("App\\Models\\Order"), "{taken}");
    let ambiguous = run(&fs, &request(Kind::Class, "Order", "Purchase"))
        .unwrap_err()
        .to_string();
    assert!(ambiguous.contains("App\\Other\\Order"), "{ambiguous}");
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn one_undo_restores_the_moved_class_and_all_references() {
    let fs = class_project();
    let before = fs.snapshot();
    run(
        &fs,
        &request(Kind::Class, "App\\Models\\Fee", "App\\Models\\Charge"),
    )
    .unwrap();
    assert!(fs.get("app/Models/Fee.php").is_none());
    undo(&fs);
    let after: BTreeMap<PathBuf, String> = fs
        .snapshot()
        .into_iter()
        .filter(|(path, _)| !path.to_string_lossy().contains(".callgraph"))
        .collect();
    assert_eq!(after, before);
}

// --------------------------------------------------------------- method

const PAY: &str = "<?php

namespace App\\Services;

class Pay
{
    public function charge(int $amount): int
    {
        return $amount;
    }

    public static function rate(): int
    {
        return 1;
    }
}
";

const PAY_CONTROLLER: &str = "<?php

namespace App\\Http\\Controllers;

use App\\Services\\Pay;

class PayController
{
    public function store(Pay $pay, $unknown)
    {
        $pay->charge(5);
        $unknown->charge(1);
        return [Pay::rate(), [Pay::class, 'charge']];
    }

    public function show()
    {
        return 'x';
    }
}
";

const PAY_ROUTES: &str = "<?php

use App\\Http\\Controllers\\PayController;
use Illuminate\\Support\\Facades\\Route;

Route::get('/pay/{id}', [PayController::class, 'show'])->name('pay.show');
Route::get('/pay2/{id}', [\\App\\Http\\Controllers\\PayController::class, 'show']);
";

fn method_project() -> MemFs {
    MemFs::with(&[
        ("composer.json", COMPOSER),
        ("app/Services/Pay.php", PAY),
        ("app/Http/Controllers/PayController.php", PAY_CONTROLLER),
        ("routes/web.php", PAY_ROUTES),
    ])
}

#[test]
fn method_rename_changes_the_declaration_resolved_calls_and_callables() {
    let fs = method_project();
    let out = run(
        &fs,
        &request(Kind::Method, "App\\Services\\Pay::charge", "bill"),
    )
    .unwrap();
    let text = out.render();
    assert!(
        text.starts_with("renamed method App\\Services\\Pay::charge → bill"),
        "{text}"
    );
    let pay = fs.get("app/Services/Pay.php").unwrap();
    assert!(
        pay.contains("public function bill(int $amount): int"),
        "{pay}"
    );
    let controller = fs.get("app/Http/Controllers/PayController.php").unwrap();
    assert!(controller.contains("$pay->bill(5);"), "{controller}");
    assert!(controller.contains("[Pay::class, 'bill']"), "{controller}");
    // a feloldatlan fogadójú hívás marad, és jelzés
    assert!(controller.contains("$unknown->charge(1);"), "{controller}");
    assert!(text.contains("PayController.php"), "{text}");
    assert!(text.contains("⚠ uncertain (not changed)"), "{text}");
}

#[test]
fn route_actions_follow_a_renamed_controller_method() {
    let fs = method_project();
    run(
        &fs,
        &request(
            Kind::Method,
            "App\\Http\\Controllers\\PayController::show",
            "display",
        ),
    )
    .unwrap();
    let routes = fs.get("routes/web.php").unwrap();
    assert!(
        routes.contains("[PayController::class, 'display']"),
        "{routes}"
    );
    assert!(
        routes.contains("[\\App\\Http\\Controllers\\PayController::class, 'display']"),
        "{routes}"
    );
    let controller = fs.get("app/Http/Controllers/PayController.php").unwrap();
    assert!(
        controller.contains("public function display()"),
        "{controller}"
    );
}

const GATEWAY: &str = "<?php

namespace App\\Pay;

interface Gateway
{
    public function charge(int $amount): int;
}
";

const STRIPE: &str = "<?php

namespace App\\Pay;

class Stripe implements Gateway
{
    public function charge(int $amount): int
    {
        return $amount;
    }
}
";

const PAYPAL: &str = "<?php

namespace App\\Pay;

class Paypal implements Gateway
{
    public function charge(int $amount): int
    {
        return $amount * 2;
    }
}
";

const CONSUMER: &str = "<?php

namespace App\\Pay;

class Consumer
{
    public function go(Gateway $gateway): int
    {
        return $gateway->charge(1);
    }
}
";

fn interface_project() -> MemFs {
    MemFs::with(&[
        ("composer.json", COMPOSER),
        ("app/Pay/Gateway.php", GATEWAY),
        ("app/Pay/Stripe.php", STRIPE),
        ("app/Pay/Paypal.php", PAYPAL),
        ("app/Pay/Consumer.php", CONSUMER),
    ])
}

#[test]
fn interface_method_rename_follows_every_implementor() {
    let fs = interface_project();
    run(
        &fs,
        &request(Kind::Method, "App\\Pay\\Gateway::charge", "bill"),
    )
    .unwrap();
    for file in ["Gateway", "Stripe", "Paypal"] {
        let text = fs.get(&format!("app/Pay/{file}.php")).unwrap();
        assert!(
            text.contains("function bill(int $amount): int"),
            "{file}: {text}"
        );
        assert!(!text.contains("charge"), "{file}: {text}");
    }
    let consumer = fs.get("app/Pay/Consumer.php").unwrap();
    assert!(consumer.contains("$gateway->bill(1)"), "{consumer}");
}

#[test]
fn renaming_an_override_alone_is_refused() {
    let fs = interface_project();
    let before = fs.snapshot();
    let err = run(
        &fs,
        &request(Kind::Method, "App\\Pay\\Stripe::charge", "bill"),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("Gateway"), "{err}");
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn method_name_collision_is_refused() {
    let fs = method_project();
    let before = fs.snapshot();
    let err = run(
        &fs,
        &request(Kind::Method, "App\\Services\\Pay::charge", "rate"),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("rate"), "{err}");
    assert_eq!(fs.snapshot(), before);
}

// ----------------------------------------------------------- the engine

#[test]
fn a_broken_result_writes_nothing() {
    let fs = MemFs::with(&[
        ("composer.json", COMPOSER),
        ("app/A.php", "<?php\n\nclass A\n{\n}\n"),
        ("app/B.php", "<?php\n\nclass B\n{\n}\n"),
    ]);
    let before = fs.snapshot();
    let at = "<?php\n\nclass A".len();
    let plan = Plan {
        sites: vec![
            Site::replace("app/B.php", 14, 15, "C"),
            Site::insert("app/A.php", at, " {{{"),
        ],
        ..Plan::default()
    };
    let clock = FixedClock(Cell::new(T0));
    let err = finish(env(&fs, &clock), "test", plan, false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("app/A.php"), "{err}");
    assert_eq!(fs.snapshot(), before);
}

#[test]
fn overlapping_edits_are_a_conflict_not_a_corruption() {
    let fs = MemFs::with(&[("app/A.php", "<?php\n\nclass A\n{\n}\n")]);
    let before = fs.snapshot();
    let plan = Plan {
        sites: vec![
            Site::replace("app/A.php", 14, 15, "B"),
            Site::replace("app/A.php", 12, 15, "C"),
        ],
        ..Plan::default()
    };
    let clock = FixedClock(Cell::new(T0));
    assert!(finish(env(&fs, &clock), "test", plan, false).is_err());
    assert_eq!(fs.snapshot(), before);
}
