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
#[ignore = "web-plan F3/H1: events are not paired with their listener; enabled in phase 6"]
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
    assert!(located.contains("config/library.php"), "{located}");
}

#[test]
#[ignore = "web-plan H10: Shelf::query()->stocked() does not resolve to the scope; enabled in phase 6 (F3)"]
fn h10_eloquent_scope_call_resolves_to_the_scope_method() {
    let out = Fixture::laravel().run(&["show", "app", "--grep", "Shelf::scopeStocked$"]);
    assert!(out.contains("ShelfController.php"), "{out}");
    assert!(out.contains("index()"), "{out}");
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
#[ignore = "web-plan H12: a TS dictionary module is not a language file; enabled in phase 6 (F6)"]
fn h12_i18n_view_reads_ts_dictionary_modules() {
    let out = Fixture::monorepo().run(&["show", ".", "--format", "i18n"]);
    assert!(!out.contains("no translation files found"), "{out}");
    let head = out.lines().next().unwrap_or("");
    assert!(
        head.starts_with("i18n lang:") && head.contains("en") && head.contains("de"),
        "{out}"
    );
}

#[test]
#[ignore = "web-plan H12: a script-side t() call is not an i18n:: key; enabled in phase 6 (F6)"]
fn h12_script_t_call_is_an_i18n_key() {
    let out = Fixture::monorepo().run(&["show", "ui/src", "--grep", "i18n::bed.locked"]);
    assert!(
        out.contains("FILE  ui/src/components/BedPlanner.vue"),
        "{out}"
    );
    assert!(out.contains("statusLine"), "{out}");
}

#[test]
#[ignore = "web-plan H12: the dictionary module outline shows no key tree; enabled in phase 6 (F6)"]
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
#[ignore = "web-plan H15: Tailwind 4 @utility is not a class definition; enabled in phase 7 (F9)"]
fn h15_tailwind_utility_is_a_class_definition() {
    let out = Fixture::laravel().run(&["show", ".", "--format", "classes"]);
    assert!(out.contains("classes: 2 defined"), "{out}");
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
#[ignore = "web-plan H18/F7: emit keys are global and the parent @listener is not an edge; enabled in phase 7"]
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
#[ignore = "web-plan H24/F4: routes generated from config are unknown to ziggy and inertia; enabled in phase 6"]
fn h24_config_generated_routes_and_pages_are_static_facts() {
    let fixture = Fixture::laravel();
    let ziggy = fixture.run(&["show", ".", "--format", "ziggy"]);
    assert!(!ziggy.contains("unknown route name 'start'"), "{ziggy}");
    assert!(ziggy.contains("routes: 5 named"), "{ziggy}");
    let inertia = fixture.run(&["show", ".", "--format", "inertia"]);
    assert!(
        lines_starting(&inertia, "unrendered").is_empty(),
        "{inertia}"
    );
}

#[test]
#[ignore = "web-plan H26/F4: the routes view lacks middleware and the full route table; enabled in phase 6 (R6)"]
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
}

#[test]
#[ignore = "web-plan H29/F10: the outline of a PHP config file shows no keys; enabled in phase 6 (F10)"]
fn h29_php_config_outline_shows_key_paths() {
    let out = Fixture::laravel().run(&["show", "config/library.php"]);
    assert!(out.contains("pages.start.uri"), "{out}");
    assert!(out.lines().count() <= 40, "{out}");
}
