//! A v0.9.2 javítások a valódi binárissal: a literál kulcsú computed hívás célpontja (JS / TS), a `--guess` az
//! egyszintű `--grep`-nél (`?` jelzéssel), és a két lábléc-sor (kihagyott név szerinti egyezések, dinamikus hívók)
//! az egyszintű útvonalon is.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-v092-it-{}-{id}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn raw(&self, args: &[&str], lang: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", lang)
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
            .env_remove("CALLGRAPH_GITIGNORE")
            .output()
            .expect("callgraph runs")
    }

    fn run(&self, args: &[&str], lang: &str) -> String {
        let out = self.raw(args, lang);
        assert!(
            out.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// `show --project <gyökér> …` angol kimenettel.
    fn show(&self, args: &[&str]) -> String {
        self.show_in(args, "en")
    }

    fn show_in(&self, args: &[&str], lang: &str) -> String {
        let root = self.0.to_string_lossy().into_owned();
        let mut all = vec!["show", "--project", root.as_str()];
        all.extend_from_slice(args);
        self.run(&all, lang)
    }
}

const SKIPPED_EN: &str = "unresolved, name-only matches skipped — --guess shows them)";
const SKIPPED_HU: &str = "feloldatlan, csak név szerinti egyezés kihagyva — --guess mutatja)";
const DYNAMIC_EN: &str = "of these callers contain dynamic calls — check them by hand)";
const DYNAMIC_HU: &str = "hívó dinamikus hívást tartalmaz — kézzel nézd át)";

const BOX: &str = "<?php\nclass Box { public function open() { return 1; } }\n";
const USE1: &str = "<?php\nclass Use1 { public function a($x) { return $x->open(); } }\n";
const USE2: &str = "<?php\nclass Use2 { public function b(Box $x) { return $x->open(); } }\n";

const WORKER: &str = "<?php\nclass Worker { public function target() { return 1; } }\n";
const RUNNER: &str = "<?php\nclass Runner {\n    public function go($m) { $w = new Worker(); $w->$m(); return $w->target(); }\n    public function clean() { $w = new Worker(); return $w->target(); }\n}\n";

fn box_project() -> Project {
    let project = Project::new();
    project.write("box.php", BOX);
    project.write("use1.php", USE1);
    project.write("use2.php", USE2);
    project
}

// ---- 1. literál kulcsú computed hívás ----

#[test]
fn literal_computed_key_gets_the_dot_access_target() {
    let project = Project::new();
    project.write(
        "d.ts",
        "export function go(o: any) { o['x'](); o.plain(); }\n",
    );
    let outline = project.show(&["d.ts"]);
    assert!(outline.contains("plain, x"), "{outline}");
    assert!(!outline.contains("[]"), "{outline}");
    assert!(!outline.contains("dynamic_call"), "{outline}");
}

#[test]
fn string_double_quoted_and_template_keys_are_the_same_as_dot_access() {
    let project = Project::new();
    project.write(
        "keys.ts",
        "export function a(o: any) { o['x'](); }\nexport function b(o: any) { o[\"x\"](); }\nexport function c(o: any) { o[`x`](); }\nexport function d(o: any) { o.x(); }\n",
    );
    let found = project.show(&[".", "--grep", "^x$"]);
    for caller in ["a()", "b()", "c()", "d()"] {
        assert!(found.contains(caller), "{caller}: {found}");
    }
    assert!(!found.contains("[]"), "{found}");
}

#[test]
fn non_literal_key_stays_dynamic_and_numeric_key_has_no_brackets() {
    let project = Project::new();
    project.write(
        "dyn.ts",
        "export function dynamic(o: any, k: string) { o[k](); }\n",
    );
    project.write("num.ts", "export function numeric(a: any[]) { a[0](); }\n");
    let outline = project.show(&["dyn.ts"]);
    assert!(outline.contains("dynamic_call"), "{outline}");
    assert!(outline.contains("o[k]"), "{outline}");
    let numeric = project.show(&["num.ts"]);
    assert!(!numeric.contains("[]"), "{numeric}");
    assert!(!numeric.contains("dynamic_call"), "{numeric}");
}

// ---- 2. --guess az egyszintű úton ----

#[test]
fn guess_shows_name_only_callers_with_a_question_mark_on_the_single_level() {
    let project = box_project();
    for args in [
        &[".", "--grep", "Box::open", "--guess"][..],
        &[".", "--grep", "Box::open", "--depth", "1", "--guess"][..],
    ] {
        let out = project.show(args);
        assert!(out.contains("Use2"), "{args:?}: {out}");
        assert!(out.contains("Use1"), "{args:?}: {out}");
        let guessed: Vec<_> = out
            .lines()
            .filter(|line| line.contains("Use1::a()"))
            .collect();
        assert_eq!(guessed.len(), 1, "{out}");
        assert!(guessed[0].ends_with("← open  ?"), "{}", guessed[0]);
        assert!(out.contains("$x->open()"), "{out}");
        assert!(out.contains("\"?\" = unresolved"), "{out}");
        // a pontos találat nem kap `?`-t
        assert!(
            out.lines()
                .filter(|line| line.contains("Use2::b()"))
                .all(|line| !line.ends_with('?')),
            "{out}"
        );
        assert!(!out.contains(SKIPPED_EN), "{out}");
    }
}

#[test]
fn without_guess_the_output_is_the_old_one_plus_the_skipped_footer() {
    let project = box_project();
    let plain = project.show(&[".", "--grep", "Box::open"]);
    assert!(plain.contains("Use2"), "{plain}");
    assert!(!plain.contains("Use1"), "{plain}");
    assert!(plain.contains(&format!("(+1 {SKIPPED_EN}")), "{plain}");
    // a lábléc elhagyásával azonos az egyszerű outline-nal (a Use2 sorai)
    let without: String = plain
        .lines()
        .filter(|line| !line.contains("name-only matches skipped"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(without.contains("Box::open"), "{without}");
    let explicit = project.show(&[".", "--grep", "Box::open", "--depth", "1"]);
    assert_eq!(plain, explicit);
}

#[test]
fn guess_with_only_name_only_callers_replaces_the_no_match_hint() {
    let project = Project::new();
    project.write("box.php", BOX);
    project.write("use1.php", USE1);
    let guessed = project.show(&[".", "--grep", "Box::open", "--guess"]);
    assert!(guessed.contains("Use1::a()"), "{guessed}");
    assert!(guessed.contains("← open  ?"), "{guessed}");
    let plain = project.show(&[".", "--grep", "Box::open"]);
    assert!(!plain.contains("Use1"), "{plain}");
    assert!(plain.contains(&format!("(+1 {SKIPPED_EN}")), "{plain}");
}

#[test]
fn guess_does_not_change_a_pattern_without_a_type() {
    let project = box_project();
    let plain = project.show(&[".", "--grep", "open"]);
    let guessed = project.show(&[".", "--grep", "open", "--guess"]);
    assert_eq!(plain, guessed);
    assert!(plain.contains("Use1") && plain.contains("Use2"), "{plain}");
    assert!(!plain.contains("name-only matches skipped"), "{plain}");
}

#[test]
fn guess_ignores_other_method_names_and_the_same_named_caller() {
    let project = Project::new();
    project.write("box.php", BOX);
    project.write(
        "other.php",
        "<?php\nclass Other {\n    public function a($x) { return $x->close(); }\n    public function open($x) { return $x->open(); }\n}\n",
    );
    let out = project.show(&[".", "--grep", "Box::open", "--guess"]);
    assert!(!out.contains("Other"), "{out}");
    let plain = project.show(&[".", "--grep", "Box::open"]);
    assert!(!plain.contains("name-only matches skipped"), "{plain}");
}

#[test]
fn guess_output_matches_the_depth_two_notation() {
    let project = box_project();
    project.write(
        "top.php",
        "<?php\nclass Top { public function t($u) { return $u->b(); } }\n",
    );
    let single = project.show(&[".", "--grep", "Box::open", "--guess"]);
    assert!(
        single.lines().any(|line| line.ends_with("← open  ?")),
        "{single}"
    );
    // a 2. szinten a név szerinti hívó ugyanígy `← név  ?` alakú
    let deep = project.show(&[".", "--grep", "Box::open", "--depth", "2", "--guess"]);
    assert!(deep.lines().any(|line| line.ends_with("← b  ?")), "{deep}");
}

// ---- 3. láblécek az egyszintű úton ----

#[test]
fn single_level_shows_the_dynamic_callers_footer() {
    let project = Project::new();
    project.write("worker.php", WORKER);
    project.write("runner.php", RUNNER);
    let line = format!("(1 {DYNAMIC_EN}");
    for args in [
        &[".", "--grep", "Worker::target"][..],
        &[".", "--grep", "Worker::target", "--depth", "1"][..],
    ] {
        let out = project.show(args);
        assert!(out.contains("Runner"), "{out}");
        assert_eq!(out.matches(&line).count(), 1, "{args:?}: {out}");
        assert!(!out.contains("name-only matches skipped"), "{out}");
        // a lábléc a záró tipp előtt áll
        assert!(out.find(&line) < out.rfind("💡"), "{out}");
    }
    // ugyanaz a szöveg a mélyebb útvonalon
    let deep = project.show(&[".", "--grep", "Worker::target", "--depth", "2"]);
    assert!(deep.contains(&line), "{deep}");
}

#[test]
fn single_level_footers_are_hungarian_too() {
    let project = Project::new();
    project.write("worker.php", WORKER);
    project.write("runner.php", RUNNER);
    project.write("box.php", BOX);
    project.write("use1.php", USE1);
    project.write("use2.php", USE2);
    let dynamic = project.show_in(&[".", "--grep", "Worker::target"], "hu");
    assert!(dynamic.contains(&format!("(1 {DYNAMIC_HU}")), "{dynamic}");
    let skipped = project.show_in(&[".", "--grep", "Box::open"], "hu");
    assert!(skipped.contains(&format!("(+1 {SKIPPED_HU}")), "{skipped}");
    let guessed = project.show_in(&[".", "--grep", "Box::open", "--guess"], "hu");
    assert!(guessed.contains("Use1::a()"), "{guessed}");
    assert!(guessed.contains("\"?\" = feloldatlan"), "{guessed}");
}

#[test]
fn footers_are_left_out_when_there_is_nothing_to_say() {
    let project = Project::new();
    project.write("worker.php", WORKER);
    project.write(
        "clean.php",
        "<?php\nclass Clean { public function run(Worker $w) { return $w->target(); } }\n",
    );
    project.write("box.php", BOX);
    project.write("use2.php", USE2);
    for pattern in ["Worker::target", "Box::open"] {
        let out = project.show(&[".", "--grep", pattern]);
        assert!(!out.contains("dynamic calls"), "{pattern}: {out}");
        assert!(
            !out.contains("name-only matches skipped"),
            "{pattern}: {out}"
        );
        let hu = project.show_in(&[".", "--grep", pattern], "hu");
        assert!(!hu.contains("dinamikus hívást"), "{pattern}: {hu}");
        assert!(!hu.contains("kihagyva"), "{pattern}: {hu}");
    }
}

#[test]
fn both_footers_appear_together_and_machine_formats_are_unchanged() {
    let project = Project::new();
    project.write("worker.php", WORKER);
    project.write("runner.php", RUNNER);
    project.write(
        "stray.php",
        "<?php\nclass Stray { public function s($x) { return $x->target(); } }\n",
    );
    let out = project.show(&[".", "--grep", "Worker::target"]);
    assert!(out.contains(&format!("(+1 {SKIPPED_EN}")), "{out}");
    assert!(out.contains(&format!("(1 {DYNAMIC_EN}")), "{out}");
    assert!(!out.contains("Stray"), "{out}");
    let json = project.show(&[".", "--format", "json"]);
    assert!(!json.contains("name-only matches skipped"), "{json}");
    assert!(!json.contains("contain dynamic calls"), "{json}");
    let stats = project.show(&[".", "--format", "summary"]);
    assert!(!stats.contains("contain dynamic calls"), "{stats}");
}
