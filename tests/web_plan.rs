//! A webes nyelvi rész javítási tervének (H1…H29) mérőalapja: a `tests/fixtures/web/` két kitalált mintaprojektjén
//! (`laravel_app`: Laravel + Inertia + Vue + Tailwind 4, `monorepo`: Vue kliens + TS szerver, két `tsconfig`)
//! minden kifejezhető hibához egy teszt, ami a JAVÍTOTT viselkedést állítja. A ma még bukó tesztek `#[ignore]`-ral
//! jelzik, melyik fázis kapcsolja be őket (`cargo test --test web_plan -- --ignored`); a már ma zöld tesztek
//! védőháló. Nem kifejezhető: H25 (belső holt absztrakció), H27 (futásidő: `tools/web_bench.sh`), H28 (nagy
//! korpusz kell: `tools/web_bench.sh`, bájt / sor oszlop).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

/// A mintaprojekt egy ideiglenes másolata (a cache a másolatba kerül, a repóba soha).
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

impl Fixture {
    fn new(name: &str) -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("callgraph-webplan-{id}-{}-{n}", std::process::id()));
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/web")
            .join(name);
        copy_dir(&source, &root);
        Self(root.canonicalize().unwrap())
    }

    fn laravel() -> Self {
        Self::new("laravel_app")
    }

    fn monorepo() -> Self {
        Self::new("monorepo")
    }

    fn tailwind() -> Self {
        Self::new("tailwind_app")
    }

    fn vuecontract() -> Self {
        Self::new("vuecontract_app")
    }

    fn command(&self, args: &[&str], project_flag: bool) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command.args(args);
        if project_flag {
            command.arg("--project").arg(&self.0);
        }
        command
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH");
        command
    }

    fn run(&self, args: &[&str]) -> String {
        let out = self.command(args, true).output().expect("callgraph runs");
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// A `--project` kapcsoló nélküli hívás kimenete és kilépési állapota (hibás hívás vizsgálatához).
    fn run_raw(&self, args: &[&str]) -> Output {
        self.command(args, false).output().expect("callgraph runs")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn lines_starting<'a>(out: &'a str, prefix: &str) -> Vec<&'a str> {
    out.lines().filter(|l| l.starts_with(prefix)).collect()
}

/// A kimenet FILE-sora a platform elválasztóját használja (Windows-on `\`).
fn sep(path: &str) -> String {
    path.replace('/', std::path::MAIN_SEPARATOR_STR)
}

fn count_files(out: &str) -> usize {
    out.lines().filter(|l| l.starts_with("FILE  ")).count()
}

// ---------------------------------------------------------------------------------------------------------------
// 0. fázis: a mintakészlet elemezhető

#[test]
fn baseline_fixtures_analyze_cleanly() {
    let laravel = Fixture::laravel();
    let out = laravel.run(&[".", "--no-cache"]);
    assert!(out.lines().count() >= 25, "laravel_app: {out}");
    // Windows-on a JSONL-ben az útvonal `\\`-sel jön; csak itt egységesítünk (a PHP-névterek miatt nem globálisan)
    let paths = out.replace("\\\\", "/");
    assert!(paths.contains("routes/web.php") && paths.contains("ShelfController.php"));

    let mono = Fixture::monorepo();
    let out = mono.run(&[".", "--no-cache"]);
    assert!(out.lines().count() >= 20, "monorepo: {out}");
    let paths = out.replace("\\\\", "/");
    assert!(paths.contains("BedPlanner.vue") && paths.contains("endpoints/beds.ts"));
}

// ---------------------------------------------------------------------------------------------------------------
// H1 – H3: regex-alapú PHP framework-nézetek

/// Egy `Event` nevű osztály `::class` hivatkozása (reláció) nem dispatch: a hivatkozás-él nem kerül a nézetekbe.
#[test]
fn class_reference_in_a_relation_is_not_an_event_dispatch() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "events"]);
    assert!(!out.contains("Book::event"), "{out}");
    assert!(!out.contains("-->  Event  "), "{out}");
}

#[test]
fn h01_events_view_has_no_noise_and_keeps_the_real_events() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "events"]);
    assert!(
        lines_starting(&out, "event ").is_empty(),
        "hamis `event` sorok:\n{out}"
    );
    assert!(out.contains("BookLoaned::dispatch"), "{out}");
    assert!(out.contains("Event::listen"), "{out}");
}

#[test]
fn h01_events_view_pairs_dispatch_with_listener() {
    let out = Fixture::laravel().run(&["show", "app", "--format", "events"]);
    let line = out
        .lines()
        .find(|l| l.contains("BookLoaned") && l.contains("SendLoanReceipt::handle"))
        .unwrap_or_else(|| panic!("nincs kiváltó + figyelő sor:\n{out}"));
    assert!(line.contains("LoanDesk::checkout"), "{line}");
}

#[test]
fn h02_middleware_label_is_separated_from_the_target() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "middleware"]);
    let rows = lines_starting(&out, "middleware");
    assert!(!rows.is_empty(), "{out}");
    for row in rows {
        assert!(
            row.as_bytes().get(10) == Some(&b' '),
            "a címke és a cél összeragad: {row}"
        );
    }
}

#[test]
fn h03_validation_view_only_shows_real_validation() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "validation"]);
    for noise in ["invalidate", "cards.renewed", "expire-reader", "Your card"] {
        assert!(!out.contains(noise), "{noise} a kimenetben:\n{out}");
    }
    assert!(out.contains("StoreShelfRequest::rules"), "{out}");
    assert!(out.contains("\"required\""), "{out}");
}

#[test]
fn h03_providers_view_only_shows_service_provider_bindings() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "providers"]);
    assert!(
        !out.contains("stocked") && !out.contains("ShelfController"),
        "{out}"
    );
    assert!(out.contains("EventServiceProvider::register"), "{out}");
}

#[test]
fn h03_jobs_view_keeps_the_queued_class_and_skips_middleware_handle() {
    // ma is zöld: védőháló az 1. fázis horgony-javításaihoz
    let out = Fixture::laravel().run(&["show", ".", "--format", "jobs"]);
    assert!(out.contains("BuildOverdueDigest"), "{out}");
    assert!(
        !out.contains("EnsureReaderCard") && !out.contains("HandleInertiaRequests"),
        "{out}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// H4 – H7: kulcsok és modul-feloldás (monorepo)

#[test]
fn h04_one_file_one_project_relative_key_across_packages() {
    let out = Fixture::monorepo().run(&[
        "show",
        ".",
        "--grep",
        "^ui/src/lib/seedsearch.ts::matchSeeds$",
    ]);
    assert!(
        out.contains(&format!("FILE  {}", sep("ui/src/lib/useReminders.ts"))),
        "{out}"
    );
    assert!(
        out.contains(&format!("FILE  {}", sep("svc/src/features/topMatch.ts"))),
        "{out}"
    );
    assert_eq!(count_files(&out), 2, "{out}");
}

#[test]
fn h05_deps_does_not_report_resolved_aliases_as_missing() {
    let out = Fixture::monorepo().run(&["deps"]);
    assert!(!out.contains("@common/calendar"), "{out}");
    assert!(!out.contains("@kit/Tag.vue"), "{out}");
}

#[test]
fn h05_alias_import_and_relative_import_share_one_key() {
    let out = Fixture::monorepo().run(&["show", ".", "--grep", "^common/calendar.ts::isSeason$"]);
    assert!(
        out.contains(&format!("FILE  {}", sep("ui/src/lib/useReminders.ts"))),
        "{out}"
    );
    assert!(
        out.contains(&format!("FILE  {}", sep("svc/src/rest.ts"))),
        "{out}"
    );
}

#[test]
fn h06_literal_dynamic_import_resolves_to_the_page() {
    let out = Fixture::monorepo().run(&["show", ".", "--grep", "views/OverviewView.vue"]);
    assert!(
        out.contains(&format!("FILE  {}", sep("ui/src/router/index.ts"))),
        "{out}"
    );
}

#[test]
fn h06_dynamic_import_of_a_package_counts_as_use() {
    let out = Fixture::monorepo().run(&["deps"]);
    assert!(!out.contains("chart.js"), "{out}");
}

#[test]
fn h07_deps_knows_the_vitest_environment_package() {
    let out = Fixture::monorepo().run(&["deps"]);
    assert!(!out.contains("jsdom"), "{out}");
}

// ---------------------------------------------------------------------------------------------------------------
// H8 – H10: hivatkozás-élek

#[test]
fn h08_function_passed_as_value_is_a_ref_edge() {
    let out = Fixture::monorepo().run(&["show", ".", "--grep", "(^|::|\\.|->)streamSensor$"]);
    assert!(
        out.contains(&format!("FILE  {}", sep("svc/src/endpoints/beds.ts"))),
        "{out}"
    );
    assert!(out.contains("(ref)"), "{out}");
    let routes = Fixture::monorepo().run(&["show", "svc/src/endpoints/beds.ts"]);
    for handler in ["streamSensor", "listBeds", "findPlants"] {
        assert!(
            routes.contains(handler),
            "{handler} a route-fájl outline-ból hiányzik:\n{routes}"
        );
    }
}

#[test]
fn h09_class_constant_reference_is_found_by_grep_and_locate() {
    let fixture = Fixture::laravel();
    let out = fixture.run(&["show", ".", "--grep", "CatalogueController"]);
    assert!(
        out.contains(&format!("FILE  {}", sep("config/library.php"))),
        "{out}"
    );
    assert!(
        out.contains(&format!("FILE  {}", sep("routes/web.php"))),
        "{out}"
    );
    let located = fixture.run(&["locate", "CatalogueController"]);
    assert!(located.contains(&sep("config/library.php")), "{located}");
}

#[test]
fn h10_eloquent_scope_call_resolves_to_the_scope_method() {
    let out = Fixture::laravel().run(&["show", "app", "--grep", "Shelf::scopeStocked$"]);
    assert!(out.contains("ShelfController.php"), "{out}");
    assert!(out.contains("index()"), "{out}");
    // a feloldott hívó mellett nem jelenik meg a „nincs találat” tipp
    assert!(!out.contains("no call target matches"), "{out}");
    assert!(!out.contains("egyetlen hívás-cél"), "{out}");
}

// ---------------------------------------------------------------------------------------------------------------
// H11 – H13: űrlap, i18n

#[test]
fn h11_forms_view_keeps_a_multiline_chain_and_an_unknown_target() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "forms"]);
    assert!(out.contains("RenewalDialog.vue"), "{out}");
    assert!(out.contains("useForm(number, pin, keep)"), "{out}");
    assert!(out.contains("POST /renewals"), "{out}");
}

#[test]
fn h12_i18n_view_reads_ts_dictionary_modules() {
    let out = Fixture::monorepo().run(&["show", ".", "--format", "i18n"]);
    assert!(!out.contains("no translation files found"), "{out}");
    let head = out.lines().next().unwrap_or("");
    assert!(
        head.starts_with("i18n ui/src/locales:") && head.contains("en") && head.contains("de"),
        "{out}"
    );
}

#[test]
fn h12_script_t_call_is_an_i18n_key() {
    let out = Fixture::monorepo().run(&["show", "ui/src", "--grep", "i18n::bed.locked"]);
    assert!(
        out.contains(&format!(
            "FILE  {}",
            sep("ui/src/components/BedPlanner.vue")
        )),
        "{out}"
    );
    assert!(out.contains("statusLine"), "{out}");
}

#[test]
fn h12_dictionary_outline_shows_the_key_tree() {
    let out = Fixture::monorepo().run(&["show", "ui/src/locales/en.ts"]);
    assert!(out.contains("locked") && out.contains("remove"), "{out}");
    assert!(out.lines().count() <= 40, "{out}");
}

#[test]
fn h13_i18n_view_keys_are_complete_and_searchable() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "i18n"]);
    assert!(!out.contains('…'), "{out}");
    assert!(out.contains("circulation.circulation.{"), "{out}");
    assert!(out.contains("lost_item_surcharge_amount"), "{out}");
}

// ---------------------------------------------------------------------------------------------------------------
// H14 – H15: CSS / Tailwind 4

#[test]
fn h14_css_var_read_keeps_the_property_name() {
    let out = Fixture::laravel().run(&["show", "resources/css/app.css"]);
    for row in out.lines() {
        let words: Vec<&str> = row.split_whitespace().collect();
        assert!(words != ["var", "var"], "a var-sor csonka:\n{out}");
    }
    assert!(out.contains("--color-parchment"), "{out}");
    assert!(out.contains("--color-binding"), "{out}");
}

#[test]
fn h14_locate_finds_the_custom_property_use() {
    let out = Fixture::laravel().run(&["locate", "--color-binding"]);
    assert!(out.contains("L19"), "a használat (L19) hiányzik:\n{out}");
}

#[test]
fn h15_tailwind_utility_is_a_class_definition() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "classes"]);
    assert!(out.contains("classes: 2 defined"), "{out}");
}

#[test]
fn f9_utility_and_theme_tokens_are_listed_in_the_classes_view() {
    let out = Fixture::tailwind().run(&["show", ".", "--format", "classes"]);
    assert!(out.contains("classes: 3 defined · 1 unused"), "{out}");
    // `@utility` classes: the files using them
    assert!(out.contains("@utility 1 used: container-page ×2"), "{out}");
    assert!(out.contains("stack-gap"), "{out}");
    // `@theme` tokens: the utility classes that templates (and `@apply`) use
    let theme = out
        .lines()
        .find(|l| l.contains("@theme 5 tokens"))
        .unwrap_or_else(|| panic!("no @theme line:\n{out}"));
    // the sample is the most used first (7 classes in use, the top 5 shown)
    assert!(
        theme.contains("used 7") && theme.contains("bg-brand-500 ×2"),
        "{theme}"
    );
    for used in ["gap-gutter", "px-gutter"] {
        assert!(theme.contains(used), "{used}: {theme}");
    }
    assert!(!out.contains("no tokens"), "{out}");
}

#[test]
fn f9_unknown_theme_token_is_flagged_but_core_classes_are_not() {
    let out = Fixture::tailwind().run(&["show", ".", "--format", "classes"]);
    let unknown = out
        .lines()
        .find(|l| l.starts_with("theme: no such token"))
        .unwrap_or_else(|| panic!("no unknown-token line:\n{out}"));
    assert!(
        unknown.contains("text-brand-450") && unknown.contains("text-brand-500"),
        "{unknown}"
    );
    for core in [
        "text-lg",
        "text-red-400",
        "p-4",
        "font-display",
        "rounded-card",
    ] {
        assert!(!unknown.contains(core), "{core}: {unknown}");
    }
}

#[test]
fn f9_stylesheet_outline_mentions_the_unknown_token_and_apply_counts_as_use() {
    let out = Fixture::tailwind().run(&["show", "src/styles/app.css"]);
    assert!(out.contains("text-brand-450"), "{out}");
    // `@apply … hover:text-brand-100` uses the token class (variant stripped)
    // 7 classes in use only when `bg-brand-500`, `px-gutter` (and `text-brand-100`) come from the `@apply`
    let view = Fixture::tailwind().run(&["show", ".", "--format", "classes"]);
    assert!(view.contains("used 7"), "{view}");
}

#[test]
fn f12_html_entry_points_are_import_lines() {
    let out = Fixture::tailwind().run(&["show", "index.html"]);
    assert!(out.contains("import /src/main.ts"), "{out}");
    assert!(out.contains("import /src/styles/app.css"), "{out}");
    assert!(out.contains("import src/legacy.js"), "{out}");
    // external URLs are not edges
    assert!(!out.contains("cdn.example.test"), "{out}");
    assert!(!out.contains("favicon"), "{out}");
}

#[test]
fn f12_html_entry_is_a_caller_of_the_module() {
    let out = Fixture::tailwind().run(&["show", ".", "--grep", "src/main\\.ts$"]);
    assert!(out.contains("index.html"), "{out}");
}

// ---------------------------------------------------------------------------------------------------------------
// H16 – H19: Vue / TS kimenet

#[test]
fn h16_vue_outline_lists_props_once_and_emits_on_one_line() {
    let out = Fixture::monorepo().run(&["show", "ui/src/components/BedPlanner.vue", "--full"]);
    assert_eq!(out.matches("focusCellId").count(), 1, "{out}");
    assert!(!out.contains("interface BedPlannerProps"), "{out}");
    let emits = out
        .lines()
        .find(|l| l.trim_start().starts_with("emits (25):"))
        .unwrap_or_else(|| panic!("nincs `emits (25): …` sor:\n{out}"));
    assert!(
        emits.contains("sowSeed") && emits.contains("updateNote"),
        "{emits}"
    );
    assert!(out.lines().count() <= 70, "{} sor", out.lines().count());
}

#[test]
fn h17_computed_value_array_method_is_not_a_vue_api() {
    let out = Fixture::laravel().run(&["show", "resources/js/pages/Shelves/Browse.vue"]);
    assert!(!out.contains("computed().value"), "{out}");
}

#[test]
fn h18_emit_key_is_component_scoped_and_wired_to_the_listener() {
    let out = Fixture::monorepo().run(&["show", "ui/src", "--grep", "CellMenu.vue::emit:remove$"]);
    assert!(
        out.contains("FILE  ui/src/components/CellMenu.vue"),
        "{out}"
    );
    assert!(
        out.contains("FILE  ui/src/components/BedPlanner.vue"),
        "{out}"
    );
    assert!(!out.contains("OverviewView.vue"), "{out}");
}

#[test]
fn h19_enum_case_read_is_not_an_external_call() {
    let out = Fixture::laravel().run(&["show", "app/Http/Controllers/ShelfController.php"]);
    for row in lines_starting(out.trim_start(), "    external_call") {
        assert!(!row.contains("LoanState::Returned"), "{row}");
    }
    assert!(
        out.contains("const_use") && out.contains("LoanState::Returned"),
        "{out}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// H20 – H23: kimenet-alak

#[test]
fn h20_grep_shows_the_line_of_the_matched_chain_call() {
    let out = Fixture::laravel().run(&["show", "app", "--grep", "(^|::|\\.|->)stocked$"]);
    assert!(out.contains("L16 │ ->stocked()"), "{out}");
}

#[test]
fn h21_unknown_flag_is_an_error_without_raw_output() {
    let out = Fixture::laravel().run_raw(&[".", "--format", "ziggy"]);
    assert!(!out.status.success(), "a hibás hívás kilépési kódja 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.lines().count() <= 5,
        "{} sor a kimeneten",
        stdout.lines().count()
    );
    assert!(!stdout.contains("{\"file\""), "nyers JSONL a kimeneten");
}

#[test]
fn h22_single_file_outline_uses_a_relative_path() {
    let out = Fixture::laravel().run(&["show", "app/Models/Shelf.php"]);
    let first = out.lines().next().unwrap();
    assert!(
        first.starts_with(&format!("FILE  {}", sep("app/Models/Shelf.php"))),
        "{first}"
    );
}

#[test]
fn h23_story_files_are_not_a_page_layer() {
    let out = Fixture::laravel().run(&["profile", "--full"]);
    let mut in_page = false;
    for row in out.lines() {
        if row.starts_with("  ") && !row.starts_with("   ") {
            in_page = row.trim_start().starts_with("page ");
        } else if in_page && row.starts_with(' ') {
            assert!(!row.contains("stories"), "a story a page rétegben:\n{out}");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// H24 – H29: route-ok, konfig

#[test]
fn h24_config_generated_routes_and_pages_are_static_facts() {
    let fixture = Fixture::laravel();
    let ziggy = fixture.run(&["show", ".", "--format", "ziggy"]);
    assert!(!ziggy.contains("unknown route name 'start'"), "{ziggy}");
    // 3 kézzel írt + 3 a `cards` csoportból (`cards.index`, a névtelen `cards.`, `cards.item.edit`) + 2 a konfigból
    assert!(ziggy.contains("routes: 8 named"), "{ziggy}");
    let inertia = fixture.run(&["show", ".", "--format", "inertia"]);
    assert!(
        lines_starting(&inertia, "unrendered").is_empty(),
        "{inertia}"
    );
}

#[test]
fn h26_routes_view_is_the_full_table_without_noise() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "routes"]);
    let rows = lines_starting(&out, "route ");
    assert!(!rows.is_empty(), "{out}");
    assert!(
        rows.iter().all(|r| !r.contains("__top__::__top__")),
        "{out}"
    );
    let reports = rows
        .iter()
        .find(|r| r.contains("/catalogue"))
        .unwrap_or_else(|| panic!("nincs /catalogue sor:\n{out}"));
    assert!(reports.contains("EnsureReaderCard"), "{reports}");
    assert!(rows.iter().any(|r| r.contains("name=start")), "{out}");
    assert!(
        rows.iter()
            .all(|r| r.contains("routes/web.php:") || r.contains(&sep("routes/web.php:"))),
        "a sorok fájl:sor helye hiányzik:\n{out}"
    );
}

/// A controller-csoportban (`->controller(X::class)->group(..)`, nyíl-függvénnyel és beágyazva is) a sztring-akció
/// `X::metódus`, és az előtag, a név és a middleware összefűződik a beágyazáson át — minden nézet ugyanazt a táblát látja.
#[test]
fn routes_in_a_controller_group_inherit_prefix_name_action_and_middleware() {
    let fixture = Fixture::laravel();
    let out = fixture.run(&["show", ".", "--format", "routes"]);
    let rows = lines_starting(&out, "route ");
    let find = |needle: &str| {
        *rows
            .iter()
            .find(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("nincs `{needle}` sor:\n{out}"))
    };
    let index = find("name=cards.index");
    assert!(index.contains("GET  /cards  "), "{index}");
    assert!(index.contains("ReaderCardController@index"), "{index}");
    assert!(index.contains("[auth, verified]"), "{index}");
    let store = find("ReaderCardController@store");
    assert!(store.contains("POST  /cards  "), "{store}");
    assert!(store.contains("name=cards."), "{store}");
    let edit = find("name=cards.item.edit");
    assert!(edit.contains("GET  /cards/{card}/edit"), "{edit}");
    assert!(edit.contains("ReaderCardController@edit"), "{edit}");
    assert!(edit.contains("[auth, verified, can:renew]"), "{edit}");
    // a ziggy nézet ugyanebből a táblából: a csoport nevei ismertek
    let ziggy = fixture.run(&["show", ".", "--format", "ziggy"]);
    assert!(!ziggy.contains("cards.index"), "{ziggy}");
}

#[test]
fn h29_php_config_outline_shows_key_paths() {
    let out = Fixture::laravel().run(&["show", "config/library.php"]);
    assert!(out.contains("pages.start.uri"), "{out}");
    assert!(out.lines().count() <= 40, "{out}");
}

/// A docblock-típusok (`@mixin`, `@var`) hívó-élei: a modell metódusát a resource és a controller is hívja.
#[test]
fn docblock_types_resolve_callers_of_a_model_method() {
    let out = Fixture::laravel().run(&["show", ".", "--grep", "Book::coverUrl"]);
    for file in [
        "app/Http/Resources/BookResource.php",
        "app/Http/Controllers/BookCoverController.php",
    ] {
        assert!(
            out.contains(&format!("FILE  {}", sep(file))),
            "{file}: {out}"
        );
    }
    // a `@var` mindkét alakja (értékadás, foreach) a controller két metódusában
    assert!(out.contains("show"), "{out}");
    assert!(out.contains("index"), "{out}");
}

// ---------------------------------------------------------------------------------------------------------------
// F8: vue-router tables, TS route tables and the client calls to them

fn has_line(out: &str, expected: &str) -> bool {
    out.lines().any(|line| line == expected)
}

#[test]
fn f8_router_table_has_nested_children_lazy_imports_and_redirects() {
    let out =
        Fixture::new("webroutes_app").run(&["show", "client/src/router", "--format", "routes"]);
    for expected in [
        "/ → client/src/pages/HomePage.vue (home)",
        "/chats → client/src/layouts/ChatLayout.vue",
        "/chats → client/src/pages/ChatListPage.vue (chat-list)",
        "/chats/:id → client/src/pages/ChatPage.vue (chat)",
        "/old-home → redirect /",
        "/start → redirect name:home",
    ] {
        assert!(has_line(&out, expected), "{expected}\n{out}");
    }
}

#[test]
fn f8_unknown_route_names_are_flagged_and_known_ones_are_not() {
    let out =
        Fixture::new("webroutes_app").run(&["show", "client/src/router", "--format", "routes"]);
    assert!(
        out.contains("unknown route name 'settings'  client/src/pages/ChatListPage.vue L12"),
        "{out}"
    );
    assert!(
        out.contains("unknown route name 'archive'  client/src/pages/ChatListPage.vue L19"),
        "{out}"
    );
    for known in ["'home'", "'chat'", "'chat-list'"] {
        assert!(
            !out.contains(&format!("unknown route name {known}")),
            "{out}"
        );
    }
    // the ziggy view is the same check without Laravel
    let ziggy = Fixture::new("webroutes_app").run(&["show", "client/src", "--format", "ziggy"]);
    assert!(ziggy.contains("unknown route name 'settings'"), "{ziggy}");
}

#[test]
fn f8_server_routes_resolve_named_and_inline_handlers() {
    let out = Fixture::new("webroutes_app").run(&["show", "server/src", "--format", "routes"]);
    assert!(
        out.contains("GET /api/chats → server/src/api.ts::listChats"),
        "{out}"
    );
    assert!(
        out.contains("GET /api/chats/:id → server/src/api.ts:13"),
        "{out}"
    );
    assert!(
        out.contains(
            "POST /api/chats/:id/messages → server/src/handlers/messages.ts::createMessage"
        ),
        "{out}"
    );
    // a namespace import member, and a sub-router mounted with a prefix
    assert!(
        out.contains("DELETE /api/chats/:id → server/src/handlers/admin.ts::removeChat"),
        "{out}"
    );
    assert!(
        out.contains("GET /api/old/archive → server/src/api.ts::listChats"),
        "{out}"
    );
}

#[test]
fn f8_regex_route_prints_as_written() {
    let out = Fixture::new("webroutes_app").run(&["show", "server/src", "--format", "routes"]);
    assert!(
        out.contains("GET ^/api/chats/(?<chatId>[^/]+)/ws$ → server/src/sockets.ts::chatSocket"),
        "{out}"
    );
    assert!(
        out.contains("GET ^/health$ → server/src/sockets.ts:14"),
        "{out}"
    );
}

#[test]
fn f8_client_urls_are_matched_to_server_routes_and_unmatched_ones_listed() {
    let out = Fixture::new("webroutes_app").run(&["show", "server/src", "--format", "routes"]);
    let list = out
        .lines()
        .find(|l| l.starts_with("GET /api/chats → "))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(
        list.contains("← client/src/composables/api.ts L2"),
        "{list}"
    );
    // the template literal `/api/chats/${id}` hits `/api/chats/:id`, the POST fetch hits only the POST route
    let one = out
        .lines()
        .find(|l| l.starts_with("GET /api/chats/:id → "))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(one.contains("api.ts L6") && !one.contains("L10"), "{one}");
    let post = out
        .lines()
        .find(|l| l.starts_with("POST /api/chats/:id/messages"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(post.contains("api.ts L10"), "{post}");
    // a literal that matches no server route; an absolute URL is never listed
    assert!(
        out.contains("no server route: GET /api/stats (client/src/composables/api.ts L14)"),
        "{out}"
    );
    assert!(!out.contains("example.com"), "{out}");
}

#[test]
fn f8_no_server_route_line_without_server_routes() {
    let out = Fixture::new("webroutes_app").run(&["show", "client/src", "--format", "routes"]);
    assert!(out.contains("/chats/:id → "), "{out}");
    assert!(!out.contains("no server route"), "{out}");
}

#[test]
fn f8_monorepo_router_and_regex_helper_routes() {
    let mono = Fixture::monorepo();
    let client = mono.run(&["show", "ui/src/router", "--format", "routes"]);
    assert!(
        has_line(&client, "/beds/:id → ui/src/views/BedView.vue (bed)"),
        "{client}"
    );
    let server = mono.run(&["show", "svc/src/endpoints", "--format", "routes"]);
    assert!(
        server.contains("GET ^/beds$ → svc/src/rest.ts::listBeds"),
        "{server}"
    );
    assert!(
        server.contains("GET ^/beds/(?<bedId>[^/]+)/sensor$ → svc/src/rest.ts::streamSensor"),
        "{server}"
    );
    assert!(!server.contains("no server route"), "{server}");
}

// ---------------------------------------------------------------------------------------------------------------
// F7: the Vue component contract (emit edges, props view, slots, provide / inject, Pinia, imported props types)

#[test]
fn f7_emit_key_has_the_emitter_and_the_parent_listener() {
    let out =
        Fixture::vuecontract().run(&["show", "src", "--grep", "MessageActions.vue::emit:purge$"]);
    assert!(out.contains("MessageActions.vue"), "{out}");
    assert!(out.contains("onSelect"), "{out}");
    assert!(out.contains("ChatView.vue"), "{out}");
    assert!(!out.contains("Panel.vue"), "{out}");
}

#[test]
fn f7_props_view_flags_missing_required_and_unknown_props() {
    let out = Fixture::vuecontract().run(&["show", "src", "--format", "props"]);
    assert!(out.starts_with("props: "), "{out}");
    assert!(
        out.lines()
            .next()
            .unwrap()
            .contains("component uses checked"),
        "{out}"
    );
    let missing: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("missing required prop 'title'"))
        .collect();
    assert_eq!(missing.len(), 1, "{out}");
    assert!(
        missing[0].contains("ChatView.vue L") && missing[0].ends_with("<MessageActions>"),
        "{out}"
    );
    assert!(out.contains("unknown prop 'colr'"), "{out}");
    assert!(out.contains("unknown prop 'cell-idx'"), "{out}");
}

#[test]
fn f7_props_view_never_flags_fallthrough_attrs_models_or_unknown_child_props() {
    let out = Fixture::vuecontract().run(&["show", "src", "--format", "props"]);
    // kebab-case == camelCase, `v-model` / `v-model:x`, HTML attributes, `data-*` / `aria-*`
    for quiet in [
        "'cell-id'",
        "'cellId'",
        "modelValue",
        "'open'",
        "'label'",
        "'id'",
        "'class'",
        "data-x",
        "aria-label",
        "'count'",
    ] {
        assert!(!out.contains(quiet), "{quiet}: {out}");
    }
    // the child reads `$attrs`, or its props type is imported: unknown attributes are not decidable
    assert!(!out.contains("'zzz'"), "{out}");
    // a `v-bind="obj"` parent can pass anything
    assert_eq!(
        out.matches("missing required prop 'tone'").count(),
        0,
        "{out}"
    );
}

#[test]
fn f7_props_view_reads_an_imported_props_type() {
    let out = Fixture::vuecontract().run(&["show", "src", "--format", "props"]);
    assert!(out.contains("missing required prop 'heading'"), "{out}");
    assert!(out.contains("<ImportedCard>"), "{out}");
    let outline = Fixture::vuecontract().run(&["show", "src/components/ImportedCard.vue"]);
    assert!(
        outline.contains("heading") && outline.contains("level"),
        "{outline}"
    );
}

#[test]
fn f7_slots_the_child_does_not_offer_are_flagged() {
    let out = Fixture::vuecontract().run(&["show", "src", "--format", "props"]);
    assert!(out.contains("unknown slot 'header'"), "{out}");
    assert!(out.contains("unknown slot 'foot'"), "{out}");
    assert!(
        !out.contains("slot 'head'") && !out.contains("slot 'footer'"),
        "{out}"
    );
    let outline = Fixture::vuecontract().run(&["show", "src/components/MessageActions.vue"]);
    assert!(outline.contains("slots (2): footer, default"), "{outline}");
}

#[test]
fn f7_outline_shows_what_define_expose_offers() {
    let outline = Fixture::vuecontract().run(&["show", "src/components/MessageActions.vue"]);
    assert!(outline.contains("exposes (1): onSelect"), "{outline}");
}

#[test]
fn f7_provide_and_inject_share_one_key() {
    let fx = Fixture::vuecontract();
    for key in ["vue::provide:ThemeKey$", "vue::provide:locale$"] {
        let out = fx.run(&["show", "src", "--grep", key]);
        assert!(
            out.contains("Panel.vue") && out.contains("ChatView.vue"),
            "{key}: {out}"
        );
    }
}

#[test]
fn f7_pinia_stores_have_an_outline_and_actions_are_call_edges() {
    let fx = Fixture::vuecontract();
    let chat = fx.run(&["show", "src/stores/chat.ts"]);
    for name in ["useChatStore", "send", "clear", "count", "items"] {
        assert!(chat.contains(name), "{name}: {chat}");
    }
    let cart = fx.run(&["show", "src/stores/cart.ts"]);
    for name in ["useCartStore", "add", "reset"] {
        assert!(cart.contains(name), "{name}: {cart}");
    }
    let view = fx.run(&["show", "src/views/ChatView.vue"]);
    for edge in [
        "useChatStore().send",
        "useChatStore().clear",
        "useCartStore().add",
    ] {
        assert!(view.contains(edge), "{edge}: {view}");
    }
}

#[test]
fn f7_auto_imported_components_bind_only_with_the_plugin() {
    let with = Fixture::vuecontract().run(&["show", "src", "--format", "props"]);
    assert!(with.contains("missing required prop 'shine'"), "{with}");
    assert!(with.contains("<Fancy> (auto)"), "{with}");
    let fx = Fixture::vuecontract();
    fs::write(
        fx.0.join("package.json"),
        "{ \"name\": \"vuecontract-app\", \"dependencies\": { \"vue\": \"^3.4.0\" } }\n",
    )
    .unwrap();
    let without = fx.run(&["show", "src", "--format", "props"]);
    assert!(
        !without.contains("shine") && !without.contains("(auto)"),
        "{without}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// F15: `trace` — the task slice of a route, controller action or page

#[test]
fn f15_trace_follows_a_route_to_rules_and_models() {
    let store = Fixture::laravel().run(&["trace", "route::shelves.store"]);
    assert!(store.lines().count() <= 30, "{store}");
    assert!(
        store.contains("route POST /shelves  name=shelves.store  routes/web.php:"),
        "{store}"
    );
    assert!(
        store.contains("action ShelfController@store  app/Http/Controllers/ShelfController.php:"),
        "{store}"
    );
    assert!(
        store.contains("rules StoreShelfRequest: stocked_at, title*"),
        "{store}"
    );
    assert!(store.contains("models Shelf("), "{store}");
}

#[test]
fn f15_trace_lists_the_middleware_of_the_route() {
    let out = Fixture::laravel().run(&["trace", "route::catalogue.show"]);
    assert!(out.contains("mw EnsureReaderCard"), "{out}");
    assert!(out.contains("action CatalogueController@__invoke"), "{out}");
}

#[test]
fn f15_trace_reaches_the_page_its_props_and_child_components() {
    let fixture = Fixture::laravel();
    for target in [
        "route::shelves.index",
        "ShelfController::index",
        "Pages/Shelves/Browse.vue",
    ] {
        let out = fixture.run(&["trace", target]);
        assert!(out.lines().count() <= 30, "{target}: {out}");
        assert!(
            out.contains("route GET /shelves  name=shelves.index"),
            "{target}: {out}"
        );
        assert!(out.contains("ShelfController@index"), "{target}: {out}");
        assert!(out.contains("Shelves/Browse.vue"), "{target}: {out}");
        assert!(out.contains("props: shelves, closed"), "{target}: {out}");
        assert!(out.contains("uses Stamp"), "{target}: {out}");
    }
}

#[test]
fn f15_trace_takes_a_verb_and_uri_or_a_bare_uri() {
    let fixture = Fixture::laravel();
    let post = fixture.run(&["trace", "route:POST:/shelves"]);
    assert!(post.contains("route POST /shelves"), "{post}");
    assert!(!post.contains("route GET /shelves"), "{post}");
    let both = fixture.run(&["trace", "/shelves"]);
    assert!(both.contains("route GET /shelves"), "{both}");
    assert!(both.contains("route POST /shelves"), "{both}");
    assert!(both.lines().count() <= 30, "{both}");
}

#[test]
fn f15_unknown_trace_target_is_one_hint_line_with_the_closest_names() {
    let out = Fixture::laravel().run(&["trace", "route::shelf.store"]);
    assert_eq!(out.lines().count(), 1, "{out}");
    assert!(out.contains("unknown target 'route::shelf.store'"), "{out}");
    assert!(out.contains("closest: "), "{out}");
    assert!(out.contains("route::shelves.store"), "{out}");
    let hint = out.split("closest: ").nth(1).unwrap_or_default();
    assert!(hint.split(", ").count() <= 5, "{out}");
    let none = Fixture::laravel().run(&["trace", "zzzzzz"]);
    assert_eq!(none.lines().count(), 1, "{none}");
    assert!(!none.contains("closest"), "{none}");
}

#[test]
fn f15_trace_of_a_js_server_route_shows_its_client_callers() {
    let out = Fixture::new("webroutes_app").run(&["trace", "GET:/api/chats"]);
    assert!(
        out.contains("GET /api/chats → server/src/api.ts::listChats"),
        "{out}"
    );
    assert!(out.contains("← client/src/composables/api.ts L2"), "{out}");
    assert!(out.lines().count() <= 30, "{out}");
}

#[test]
fn f15_trace_of_a_path_reaches_a_regex_server_route() {
    // `route("GET", /^\/health$/, …)`: the plain request path finds the regex route
    let out = Fixture::new("webroutes_app").run(&["trace", "GET:/health"]);
    assert!(
        out.contains("GET ^/health$ → server/src/sockets.ts"),
        "{out}"
    );
    assert!(!out.contains("unknown target"), "{out}");
}

// ---------------------------------------------------------------------------------------------------------------
// F14: diagnostics in the view footers

#[test]
fn f14_footer_names_what_a_view_skipped() {
    let fixture = Fixture::new("diag_app");
    let inertia = fixture.run(&["show", "app", "--format", "inertia"]);
    assert!(
        inertia.contains("? 1 skipped: page name is not a literal"),
        "{inertia}"
    );
    assert!(inertia.contains("PageController.php L"), "{inertia}");
    let routes = fixture.run(&["show", "routes", "--format", "routes"]);
    assert!(
        routes.contains("skipped: route name built at run time"),
        "{routes}"
    );
    assert!(routes.contains("routes/web.php L"), "{routes}");
    let props = Fixture::vuecontract().run(&["show", "src", "--format", "props"]);
    assert!(props.contains("skipped: v-bind object"), "{props}");
    assert!(props.contains("ChatView.vue L"), "{props}");
}

#[test]
fn f14_no_footer_when_nothing_was_skipped() {
    let fixture = Fixture::laravel();
    for view in ["inertia", "routes", "forms", "props", "ziggy"] {
        let out = fixture.run(&["show", ".", "--format", view]);
        assert!(!out.contains("skipped:"), "{view}: {out}");
        assert!(!out.contains("kihagyva:"), "{view}: {out}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// F11: Blade extras

#[test]
fn f11_missing_include_and_component_are_flagged_namespaced_ones_are_not() {
    let out = Fixture::new("blade_app").run(&["show", "resources/views/page.blade.php"]);
    assert!(
        out.contains("⚠ missing view: partials.gone (@include L4), components.ghost (<x-…> L8)"),
        "{out}"
    );
    // a package-namespaced view, an existing include / component / layout: never missing
    let flagged: Vec<&str> = out.lines().filter(|l| l.contains("missing view")).collect();
    for fine in [
        "mail::message",
        "partials.row",
        "components.card",
        "layouts.app",
    ] {
        assert!(!flagged.iter().any(|l| l.contains(fine)), "{fine}: {out}");
    }
}

#[test]
fn f11_dynamic_component_is_noted_not_flagged() {
    let out = Fixture::new("blade_app").run(&["show", "resources/views/page.blade.php"]);
    assert!(
        out.contains("<x-dynamic-component>, target is a run-time value: L9"),
        "{out}"
    );
    assert!(!out.contains("components.dynamic-component"), "{out}");
}

#[test]
fn f11_custom_directive_is_listed_at_the_provider_and_linked_from_the_template() {
    let fixture = Fixture::new("blade_app");
    let page = fixture.run(&["show", "resources/views/page.blade.php"]);
    assert!(
        page.contains("directives: @datetime → app/Providers/AppServiceProvider.php L12"),
        "{page}"
    );
    // Alpine's `@click` is not a registered directive
    assert!(!page.contains("@click →"), "{page}");
    let provider = fixture.run(&["show", "app/Providers/AppServiceProvider.php"]);
    assert!(
        provider.contains("blade directives: @datetime L12 → page"),
        "{provider}"
    );
}

#[test]
fn f11_vite_entries_are_import_edges_to_the_js_entry() {
    let fixture = Fixture::new("blade_app");
    let js = fixture.run(&["show", ".", "--grep", "resources/js/app.ts"]);
    assert!(
        js.contains("layouts/app.blade.php") || js.contains("layouts\\app.blade.php"),
        "{js}"
    );
    let css = fixture.run(&["show", ".", "--grep", "resources/css/app.css"]);
    assert!(css.contains("app.blade.php"), "{css}");
}
