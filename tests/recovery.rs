//! A helyreállító üzenetek a valódi binárissal, egy kis PHP-projekten: találat nélküli `--grep`, `--strings`,
//! `locate`; a `Not found` / `No such method` javaslat-sora; ismeretlen `--category`; a `--depth` „nincs hívó”
//! két változata. Találatnál nincs tipp, a gépi formátumok (json, stats, mermaid, dot) változatlanok, és a
//! projekt-gráffal és nélküle (`CALLGRAPH_NO_GRAPH=1`) azonos a kimenet. Mindkét nyelven.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const USER: &str = "<?php\nnamespace App\\Models;\n\nclass User\n{\n    public static function find($id)\n    {\n    }\n\n    public function save()\n    {\n    }\n}\n";
const POST: &str = "<?php\nnamespace App\\Models;\n\nclass Post\n{\n    public function publish()\n    {\n    }\n\n    public function greet()\n    {\n        return 'Hello World';\n    }\n}\n";
const CONTROLLER: &str = "<?php\nnamespace App\\Http;\n\nuse App\\Models\\User;\n\nclass UserController\n{\n    public function show($id)\n    {\n        return User::find($id);\n    }\n}\n";
const ORPHAN: &str = "<?php\nnamespace App\\Services;\n\nclass Orphan\n{\n    public function lonely()\n    {\n    }\n}\n";

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-recovery-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        for (path, text) in [
            ("app/Models/User.php", USER),
            ("app/Models/Post.php", POST),
            ("app/Http/UserController.php", CONTROLLER),
            ("app/Services/Orphan.php", ORPHAN),
        ] {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self(root.canonicalize().unwrap())
    }

    /// Egy futás: `args` (az első a parancs), a `--project` a projekt; `lang`: `en` / `hu`; `graph`: false =
    /// `CALLGRAPH_NO_GRAPH=1`.
    fn command(&self, args: &[&str], lang: &str, graph: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", lang)
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH");
        if !graph {
            command.env("CALLGRAPH_NO_GRAPH", "1");
        }
        command.output().expect("callgraph runs")
    }

    /// Sikeres futás kimenete (stdout, `/`-jelekkel).
    fn ok(&self, args: &[&str], lang: &str) -> String {
        let out = self.command(args, lang, true);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().replace('\\', "/")
    }

    /// Sikertelen futás: (kilépési kód, stderr sorai, `/`-jelekkel).
    fn fail(&self, args: &[&str], lang: &str) -> (i32, Vec<String>) {
        let out = self.command(args, lang, true);
        assert!(!out.status.success(), "{args:?} should fail");
        let lines = String::from_utf8(out.stderr)
            .unwrap()
            .replace('\\', "/")
            .lines()
            .map(str::to_owned)
            .collect();
        (out.status.code().unwrap(), lines)
    }

    /// A gráfos és a gráf nélküli (`CALLGRAPH_NO_GRAPH=1`) kimenet azonos; visszaadja.
    fn same_with_and_without_graph(&self, args: &[&str], lang: &str) -> String {
        let with = self.command(args, lang, true);
        let without = self.command(args, lang, false);
        assert!(with.status.success() && without.status.success(), "{args:?}");
        assert_eq!(with.stdout, without.stdout, "{args:?}");
        String::from_utf8(with.stdout).unwrap().replace('\\', "/")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const GENERIC_TIP_EN: &str = "for the body logic";
const GENERIC_TIP_HU: &str = "a törzs-logikához";

#[test]
fn grep_without_a_match_explains_the_pattern_and_suggests_the_next_step() {
    let project = Project::new();
    let out = project.same_with_and_without_graph(&["show", "app", "--grep", "Usr::find"], "en");
    assert!(
        out.starts_with("ℹ no call target matches Usr::find\n"),
        "{out}"
    );
    assert!(out.contains("regular expression on the call TARGET"), "{out}");
    assert!(out.contains("(^|::|\\.|->)name$"), "{out}");
    assert!(out.contains("--guess"), "{out}");
    assert!(out.contains("Type::method"), "{out}");
    // az azonosítóból kinyert név szerinti minta, és a hasonló nevű metódus
    assert!(out.contains("by name only: '(^|::|\\.|->)find$'"), "{out}");
    assert!(out.contains("did you mean: User::find"), "{out}");
    // a generikus „törzs-logika” tipp ilyenkor nem jelenhet meg
    assert!(!out.contains(GENERIC_TIP_EN), "{out}");
    assert!(!out.contains("FILE  "), "{out}");

    let hu = project.same_with_and_without_graph(&["show", "app", "--grep", "Usr::find"], "hu");
    assert!(
        hu.starts_with("ℹ egyetlen hívás-cél sem illeszkedik a mintára: Usr::find\n"),
        "{hu}"
    );
    assert!(hu.contains("reguláris kifejezés a hívás CÉLJÁRA"), "{hu}");
    assert!(hu.contains("esetleg erre gondoltál: User::find"), "{hu}");
    assert!(!hu.contains(GENERIC_TIP_HU), "{hu}");
}

#[test]
fn grep_hint_adapts_to_the_pattern() {
    let project = Project::new();
    // elgépelt metódusnév: a név szerinti minta ugyanaz, mint amit a skill ajánl, ezért nem ismétli
    let by_name = project.ok(&["show", "app", "--grep", r"(^|::|\.|->)fnd$"], "en");
    assert!(by_name.starts_with("ℹ no call target matches "), "{by_name}");
    assert!(!by_name.contains("by name only"), "{by_name}");
    assert!(by_name.contains("did you mean: User::find"), "{by_name}");
    // elgépelt típus
    let typo = project.ok(&["show", "app", "--grep", "Pst::publish"], "en");
    assert!(typo.contains("did you mean: Post::publish"), "{typo}");
    // összetett reguláris kifejezés: magyarázat van, kinyert név és javaslat nincs
    let regex = project.ok(&["show", "app", "--grep", "^zork(a|b)$"], "en");
    assert!(regex.starts_with("ℹ no call target matches "), "{regex}");
    assert!(!regex.contains("by name only") && !regex.contains("did you mean"), "{regex}");
    // semmire sem hasonlító név
    let nothing = project.ok(&["show", "app", "--grep", "zork"], "en");
    assert!(nothing.contains("by name only: '(^|::|\\.|->)zork$'"), "{nothing}");
    assert!(!nothing.contains("did you mean"), "{nothing}");
    // a javaslat legfeljebb 3 név
    let many = project.ok(&["show", "app", "--grep", "Usr::sav"], "en");
    let suggestion = many.lines().find(|l| l.contains("did you mean")).unwrap_or("");
    assert!(suggestion.matches("::").count() <= 3, "{many}");
}

#[test]
fn grep_with_a_match_has_no_recovery_hint() {
    let project = Project::new();
    for pattern in ["User::find", r"(^|::|\.|->)find$"] {
        let out = project.same_with_and_without_graph(&["show", "app", "--grep", pattern], "en");
        assert!(out.contains("FILE  "), "{out}");
        assert!(out.contains("UserController"), "{out}");
        assert!(out.contains(GENERIC_TIP_EN), "{out}");
        for hint in ["ℹ", "no call target matches", "did you mean", "by name only"] {
            assert!(!out.contains(hint), "{pattern}: {hint}\n{out}");
        }
    }
    let hu = project.ok(&["show", "app", "--grep", "User::find"], "hu");
    assert!(hu.contains(GENERIC_TIP_HU), "{hu}");
    assert!(!hu.contains("esetleg erre gondoltál"), "{hu}");
}

#[test]
fn not_found_path_gets_a_second_line_with_similar_paths() {
    let project = Project::new();
    // azonos fájlnév más mappában
    let (code, lines) = project.fail(&["show", "app/Model/User.php"], "en");
    assert_eq!(code, 2);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].starts_with("Not found: "), "{lines:?}");
    assert!(lines[0].contains("(relative to the --project "), "{lines:?}");
    assert!(lines[0].ends_with(" root)"), "{lines:?}");
    assert_eq!(lines[1], "Did you mean: app/Models/User.php");
    // elgépelt fájlnév ugyanabban a mappában
    let (_, lines) = project.fail(&["show", "app/Models/Usr.php"], "en");
    assert_eq!(lines[1], "Did you mean: app/Models/User.php", "{lines:?}");
    // elgépelt mappa
    let (_, lines) = project.fail(&["show", "app/Model"], "en");
    assert_eq!(lines[1], "Did you mean: app/Models", "{lines:?}");
    // nincs hasonló: csak az első sor, változatlanul
    let (code, lines) = project.fail(&["show", "zzz/qqq.php"], "en");
    assert_eq!(code, 2);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("Not found: "), "{lines:?}");
    // magyarul
    let (_, lines) = project.fail(&["show", "app/Model/User.php"], "hu");
    assert!(lines[0].starts_with("Nem található: "), "{lines:?}");
    assert_eq!(lines[1], "Esetleg erre gondoltál: app/Models/User.php");
}

#[test]
fn not_found_keeps_its_class_through_the_skill_wrapper_and_the_stats() {
    let project = Project::new();
    let skill = project.0.join(".skill");
    fs::create_dir_all(&skill).unwrap();
    let stats = project.0.join(".stats");
    fs::create_dir_all(&stats).unwrap();
    // a `wrap` a skill-szkript belépője: felhasználói hiba, csak az üzenet (error-id és napló nélkül)
    let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .args(["wrap", "app/Model/User.php", "--project"])
        .arg(&project.0)
        .current_dir(&project.0)
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_UPDATE_CHECK", "0")
        .env("CALLGRAPH_STATS", "0")
        .env("CALLGRAPH_SKILL_DIR", &skill)
        .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).unwrap().replace('\\', "/");
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 2, "{err}");
    assert!(lines[0].starts_with("Not found: "), "{err}");
    assert_eq!(lines[1], "Did you mean: app/Models/User.php");
    assert!(!err.contains("error-id"), "{err}");
    assert!(!skill.join("errors.jsonl").exists());

    // a statisztikában továbbra is egy hiba a `show` sorában (a javaslat-sor nem rontja el az osztályozást)
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .current_dir(&project.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env("CALLGRAPH_SKILL_DIR", &skill)
            .env("CALLGRAPH_STATS_DIR", &stats)
            .env_remove("CALLGRAPH_STATS")
            .env_remove("CALLGRAPH_STATS_CLIENT")
            .output()
            .unwrap()
    };
    assert!(!run(&["show", "app/Model/User.php"]).status.success());
    assert!(run(&["show", "app/Models/User.php"]).status.success());
    let report = run(&["stats", "--days", "3"]);
    let report = String::from_utf8(report.stdout).unwrap();
    let show = report
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .find(|cells| cells.first() == Some(&"show"))
        .unwrap_or_else(|| panic!("no show row: {report}"));
    assert_eq!(show[1..3], ["2", "1"], "{report}");
}

#[test]
fn no_such_method_gets_a_did_you_mean_line() {
    let project = Project::new();
    let (code, lines) = project.fail(&["show", "app/Models/User.php::fnd"], "en");
    assert_eq!(code, 2);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].starts_with("No such method: "), "{lines:?}");
    assert!(lines[0].contains("User.php::fnd"), "{lines:?}");
    assert!(lines[0].ends_with("(see --format outline for the file's full list)"), "{lines:?}");
    assert_eq!(lines[1], "Did you mean: find");
    // kis-/nagybetű-eltérés és előtag
    let (_, lines) = project.fail(&["show", "app/Models/User.php::Save"], "en");
    assert_eq!(lines[1], "Did you mean: save", "{lines:?}");
    let (_, lines) = project.fail(&["show", "app/Models/Post.php::pub"], "en");
    assert_eq!(lines[1], "Did you mean: publish", "{lines:?}");
    // nincs hasonló: csak az első sor
    let (_, lines) = project.fail(&["show", "app/Models/User.php::zzzzzz"], "en");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("No such method: "), "{lines:?}");
    // magyarul
    let (_, lines) = project.fail(&["show", "app/Models/User.php::fnd"], "hu");
    assert!(lines[0].starts_with("Nincs ilyen metódus: "), "{lines:?}");
    assert_eq!(lines[1], "Esetleg erre gondoltál: find");
    // a létező metódus továbbra is működik
    let body = project.ok(&["show", "app/Models/User.php::find", "--body"], "en");
    assert!(body.contains("find"), "{body}");
}

#[test]
fn unknown_category_is_a_usage_error_listing_the_known_ones() {
    let project = Project::new();
    let (code, lines) = project.fail(&["show", "app", "--category", "bogus_use"], "en");
    assert_eq!(code, 2);
    assert!(
        lines[0].starts_with("Unknown category: bogus_use (known: "),
        "{lines:?}"
    );
    // az ismert lista tartalmazza az új dynamic_call-t is, a PHP és a többi nyelv kategóriáit
    for known in [
        "model_use",
        "orm_relation",
        "external_call",
        "native_rust",
        "native_python",
        "native_go",
        "swiftui_view",
        "vue_emit",
        "dynamic_call",
    ] {
        assert!(lines[0].contains(known), "{known}: {lines:?}");
    }
    assert!(lines[0].ends_with("dynamic_call)"), "{lines:?}");
    // elgépelés: javaslat
    let (_, lines) = project.fail(&["show", "app", "--category", "modle_use"], "en");
    assert!(lines[1].starts_with("Did you mean: "), "{lines:?}");
    assert!(lines[1].contains("model_use"), "{lines:?}");
    // a lista egyik tagja ismeretlen: a hiba azt nevezi meg
    let (code, lines) = project.fail(
        &["show", "app", "--category", "model_use,bogus", "--format", "json"],
        "en",
    );
    assert_eq!(code, 2);
    assert!(lines[0].starts_with("Unknown category: bogus (known: "), "{lines:?}");
    // magyarul
    let (_, lines) = project.fail(&["show", "app", "--category", "bogus_use"], "hu");
    assert!(
        lines[0].starts_with("Ismeretlen kategória: bogus_use (ismertek: "),
        "{lines:?}"
    );
    // az ismert kategóriák (az új dynamic_call is) továbbra is működnek
    for category in ["dynamic_call", "external_call", "model_use,orm_relation"] {
        project.ok(&["show", "app", "--category", category], "en");
    }
}

#[test]
fn locate_without_a_match_suggests_other_ways_to_ask() {
    let project = Project::new();
    let out = project.ok(&["locate", "zzzunknownzzz"], "en");
    assert!(out.contains("no match in the project map"), "{out}");
    assert!(out.contains("no match: zzzunknownzzz\n"), "{out}");
    let tip = out.lines().find(|l| l.starts_with("💡 ")).unwrap_or("");
    for needle in ["part of the name", "camelCase", "snake_case", "singular", "visszatérítés → refund", "--strings"] {
        assert!(tip.contains(needle), "{needle}: {out}");
    }
    // egy találatos és egy találat nélküli kifejezés: a találat nélkülihez szól a tipp
    let partial = project.ok(&["locate", "User", "zzzunknownzzz"], "en");
    assert!(partial.contains("1. "), "{partial}");
    assert!(partial.contains("no match: zzzunknownzzz\n💡 "), "{partial}");
    // minden kifejezés talál: nincs tipp
    let found = project.ok(&["locate", "User"], "en");
    assert!(!found.contains("💡") && !found.contains("no match"), "{found}");
    // magyarul, és a gráf nélküli út is azonos
    let hu = project.ok(&["locate", "zzzunknownzzz"], "hu");
    assert!(hu.contains("nincs találat: zzzunknownzzz\n💡 próbáld a név egy részét"), "{hu}");
    let with = project.command(&["locate", "zzzunknownzzz"], "en", true);
    let without = project.command(&["locate", "zzzunknownzzz"], "en", false);
    assert_eq!(with.stdout, without.stdout);
}

#[test]
fn strings_without_a_match_explains_the_matching_rule() {
    let project = Project::new();
    let out = project.ok(&["show", "app", "--strings", "nothing-like-this"], "en");
    assert_eq!(
        out,
        "(no matches)\n💡 the match is a case-insensitive substring of the string literal (not a regex); try a shorter fragment\n"
    );
    // a tipp igaz: a kis-/nagybetű nem számít, a részlet elég
    for fragment in ["hello", "HELLO WORLD", "lo wo"] {
        let found = project.ok(&["show", "app", "--strings", fragment], "en");
        assert!(found.contains("Hello World"), "{fragment}: {found}");
        assert!(!found.contains("💡") && !found.contains("(no matches)"), "{found}");
    }
    // a hosszabb, el nem találó szöveg ugyanazt a tippet adja, a rövidebb töredék pedig találatot
    let miss = project.ok(&["show", "app", "--strings", "hello world!"], "en");
    assert!(miss.starts_with("(no matches)\n💡 "), "{miss}");
    let hu = project.ok(&["show", "app", "--strings", "nothing-like-this"], "hu");
    assert!(hu.starts_with("(nincs találat)\n💡 az illesztés a string-literál kis-/nagybetű-független részlete"), "{hu}");
    // a --const üzenete változatlan
    let constants = project.ok(&["show", "app", "--const", "NOTHING_LIKE_THIS"], "en");
    assert_eq!(constants, "(no matches)\n");
}

#[test]
fn depth_tells_a_missing_target_from_an_uncalled_one() {
    let project = Project::new();
    let level1 = |out: &str| {
        out.split("== level 2")
            .next()
            .unwrap_or_default()
            .to_owned()
    };

    // nincs ilyen metódus: helyesírás-ellenőrzés és javaslat
    let missing = project.same_with_and_without_graph(
        &["show", "app", "--grep", "Usr::find", "--depth", "2"],
        "en",
    );
    let first = level1(&missing);
    assert!(first.starts_with("== level 1 — directly calls: /Usr::find/\n"), "{missing}");
    assert!(
        first.contains("  (no method named Usr::find in the project map — check the spelling)\n"),
        "{missing}"
    );
    assert!(first.contains("  💡 did you mean: User::find\n"), "{missing}");
    assert!(!first.contains("no more callers"), "{missing}");
    assert!(missing.contains("== level 2"), "{missing}");

    // létezik, de senki sem hívja
    let orphan = project.same_with_and_without_graph(
        &["show", "app", "--grep", "Orphan::lonely", "--depth", "2"],
        "en",
    );
    let first = level1(&orphan);
    assert!(
        first.contains("  (Orphan::lonely exists but nothing calls it in the project — it may be an entry point, an event/route handler or called dynamically)\n"),
        "{orphan}"
    );
    assert!(!first.contains("no method named") && !first.contains("no more callers"), "{orphan}");

    // név szerinti minta: a név kerül az üzenetbe
    let by_name = project.same_with_and_without_graph(
        &["show", "app", "--grep", r"(^|::|\.|->)lonely$", "--depth", "2"],
        "en",
    );
    assert!(by_name.contains("  (lonely exists but nothing calls it in the project"), "{by_name}");

    // máshol hívják (a vizsgált mappában nem): a gyökérből kell keresni
    let elsewhere = project.same_with_and_without_graph(
        &["show", "app/Models", "--grep", "User::find", "--depth", "2"],
        "en",
    );
    assert!(
        level1(&elsewhere).contains("  (User::find is not called from this path, but it is called elsewhere in the project — search from the project root)\n"),
        "{elsewhere}"
    );

    // találat: nincs tipp, a megszokott nézet
    let hit = project.same_with_and_without_graph(
        &["show", "app", "--grep", "User::find", "--depth", "2"],
        "en",
    );
    assert!(hit.contains("UserController::show()"), "{hit}");
    for text in ["no method named", "exists but nothing calls", "did you mean", "not called from this path"] {
        assert!(!hit.contains(text), "{text}\n{hit}");
    }

    // magyarul
    let hu = project.ok(&["show", "app", "--grep", "Usr::find", "--depth", "2"], "hu");
    assert!(
        hu.contains("  (nincs Usr::find nevű metódus a projekt-térképen — ellenőrizd a helyesírást)\n"),
        "{hu}"
    );
    assert!(hu.contains("  💡 esetleg erre gondoltál: User::find\n"), "{hu}");
    let hu = project.ok(&["show", "app", "--grep", "Orphan::lonely", "--depth", "2"], "hu");
    assert!(hu.contains("  (Orphan::lonely létezik, de a projektben semmi sem hívja"), "{hu}");
}

#[test]
fn machine_formats_and_next_lines_carry_no_hint() {
    let project = Project::new();
    let json = project.ok(&["show", "app/Models/User.php", "--format", "json"], "en");
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(value.is_array(), "{json}");
    for format in ["json", "stats", "mermaid", "dot"] {
        for target in ["app", "app/Models/User.php"] {
            let out = project.ok(&["show", target, "--format", format], "en");
            for hint in ["💡", "ℹ", "Did you mean", "did you mean", "no call target"] {
                assert!(!out.contains(hint), "{format} {target}: {hint}\n{out}");
            }
        }
    }
    // üres gépi kimenet (nincs dynamic_call): továbbra is tiszta JSON, tipp nélkül
    let empty = project.ok(&["show", "app", "--format", "json", "--category", "dynamic_call"], "en");
    serde_json::from_str::<serde_json::Value>(&empty).expect("valid JSON");
    assert!(!empty.contains("💡") && !empty.contains("ℹ"), "{empty}");
    // a stats-ban a kategória-számláló nem tartalmaz tipp-sort
    let stats = project.ok(&["show", "app", "--format", "stats"], "en");
    assert!(stats.lines().all(|line| !line.starts_with("💡")), "{stats}");
    // a hívók gépi (JSONL) parancsa nulla találatnál üres marad
    let callers = project.command(&["callers", "Usr::find"], "en", true);
    assert!(callers.status.success());
    assert!(callers.stdout.is_empty(), "{}", String::from_utf8_lossy(&callers.stdout));
}
