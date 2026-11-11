//! A formázók kétnyelvű szövegei: angol alapból, magyar a `Lang::Hu`-val (byte-ra a korábbi szöveg),
//! a kimenet szerkezete (sorok száma) nyelvtől független.

use super::FormatterRegistry;
use crate::core::SourceFile;
use crate::i18n::Lang;
use crate::lang::LanguageRegistry;
use crate::query::{OutputLimit, QueryResult};

const CLASS: &str = "<?php\nnamespace App;\n\nclass Shop\n{\n    public const LIMIT = 5;\n    public int $count = 0;\n\n    public function run(int $x): int\n    {\n        if ($x > 1) {\n            return strlen('a');\n        }\n        return 0;\n    }\n}\n";
const SCRIPT: &str = "<?php\nif ($a) {\n    echo strlen('x');\n}\n";

fn row(path: &str, text: &str) -> QueryResult {
    let language = LanguageRegistry::with_defaults();
    let php = language.for_path(std::path::Path::new(path)).unwrap();
    let graph = php
        .analyze(&SourceFile::new(path, text.as_bytes().to_vec(), None))
        .unwrap()
        .graph;
    QueryResult::new(graph, None)
}

fn render(lang: Lang, name: &str, rows: &[QueryResult]) -> String {
    FormatterRegistry::with_defaults()
        .in_lang(lang)
        .render_with(name, rows, None, Some("no-such-needle"))
        .unwrap()
}

/// Mindkét nyelven renderel, ellenőrzi az azonos sorszámot, és visszaadja a két szöveget.
fn both(name: &str, rows: &[QueryResult]) -> (String, String) {
    let (en, hu) = (render(Lang::En, name, rows), render(Lang::Hu, name, rows));
    assert_eq!(
        en.lines().count(),
        hu.lines().count(),
        "{name}:\n{en}\n{hu}"
    );
    (en, hu)
}

#[test]
fn api_labels_follow_the_language() {
    let (en, hu) = both("api", &[row("src/Shop.php", CLASS)]);
    assert!(en.contains("  public constants (1):\n"), "{en}");
    assert!(en.contains("  public properties (1):\n"), "{en}");
    assert!(en.contains("  public API (1):\n"), "{en}");
    assert!(
        en.ends_with("For the call graph: --format outline.\n"),
        "{en}"
    );
    assert!(hu.contains("  publikus konstansok (1):\n"), "{hu}");
    assert!(hu.contains("  publikus property-k (1):\n"), "{hu}");
    assert!(hu.contains("  publikus felület (1):\n"), "{hu}");
    assert!(hu.ends_with("Hívás-gráfhoz: --format outline.\n"), "{hu}");
}

#[test]
fn stats_labels_follow_the_language() {
    let (en, hu) = both("stats", &[row("src/Shop.php", CLASS)]);
    for label in [
        "containers: 1 (class)",
        "properties: 1",
        "constants: 1",
        "methods: 1",
    ] {
        assert!(en.contains(label), "{label}: {en}");
    }
    assert!(
        en.contains("total calls: ") && en.contains("by category:\n"),
        "{en}"
    );
    for label in [
        "containerek: 1 (class)",
        "property-k: 1",
        "konstansok: 1",
        "metódusok: 1",
    ] {
        assert!(hu.contains(label), "{label}: {hu}");
    }
    assert!(
        hu.contains("hívások összesen: ") && hu.contains("kategória szerint:\n"),
        "{hu}"
    );
}

#[test]
fn summary_and_coverage_follow_the_language() {
    let rows = [row("src/Shop.php", CLASS), row("src/run.php", SCRIPT)];
    let (en, hu) = both("summary", &rows);
    assert!(en.contains("Shop.php  class Shop  (1 method)\n"), "{en}");
    assert!(en.contains("run.php  procedural ("), "{en}");
    assert!(hu.contains("Shop.php  class Shop  (1 metódus)\n"), "{hu}");
    assert!(hu.contains("run.php  procedurális ("), "{hu}");
    let (en, hu) = both("coverage", &rows);
    assert!(
        en.starts_with("coverage: 0/1 (0%)\n\npublic methods WITHOUT a docblock:\n"),
        "{en}"
    );
    assert!(
        hu.starts_with("lefedettség: 0/1 (0%)\n\ndocblock NÉLKÜLI publikus metódusok:\n"),
        "{hu}"
    );
}

#[test]
fn procedural_outline_follows_the_language() {
    let (en, hu) = both("outline", &[row("src/run.php", SCRIPT)]);
    assert!(
        en.contains("(procedural file — no class/interface/trait)\n"),
        "{en}"
    );
    assert!(
        en.contains("  conditionals:\n") && en.contains("  branches: L"),
        "{en}"
    );
    assert!(en.contains("  calls in order:\n"), "{en}");
    assert!(
        hu.contains("(procedurális fájl — nincs class/interface/trait)\n"),
        "{hu}"
    );
    assert!(
        hu.contains("  elágazások:\n") && hu.contains("  ágak: L"),
        "{hu}"
    );
    assert!(hu.contains("  hívások sorrendben:\n"), "{hu}");
}

#[test]
fn empty_results_follow_the_language() {
    let rows = [row("src/Shop.php", CLASS)];
    assert_eq!(
        both("const", &rows),
        ("(no matches)\n".into(), "(nincs találat)\n".into())
    );
    // a `--strings` nulla találatánál a „nincs találat” után az illesztési szabály és a következő lépés is áll
    let (en, hu) = both("strings", &rows);
    assert!(en.starts_with("(no matches)\n💡 "), "{en}");
    assert!(en.contains("case-insensitive substring"), "{en}");
    assert!(hu.starts_with("(nincs találat)\n💡 "), "{hu}");
    assert!(hu.contains("kis-/nagybetű-független részlete"), "{hu}");
    let script = [row("src/plain.php", "<?php\necho 1;\n")];
    let (en, hu) = both("flow", &script);
    assert!(
        en.starts_with("(no if/match/switch branches in the selected part"),
        "{en}"
    );
    assert!(
        hu.starts_with("(nincs if/match/switch elágazás a kiválasztott részben"),
        "{hu}"
    );
    let (en, hu) = both("method-index", &rows);
    assert!(en.contains("  run()  L9-15  [public]  calls: "), "{en}");
    assert!(hu.contains("  run()  L9-15  [public]  hívás: "), "{hu}");
}

#[test]
fn unknown_format_error_follows_the_language() {
    let error = |lang| {
        FormatterRegistry::with_defaults()
            .in_lang(lang)
            .render("nope", &[])
            .unwrap_err()
            .to_string()
    };
    assert!(error(Lang::En).contains("Unknown format: nope ("));
    assert!(error(Lang::Hu).contains("Ismeretlen formátum: nope ("));
}

#[test]
fn output_limit_notices_follow_the_language() {
    let limit = |lang| {
        OutputLimit::from_env(false)
            .with_value(Some(1))
            .with_lang(lang)
    };
    let index = |lang: Lang| limit(lang).method_index("a\nb".into(), "IDX".into());
    assert!(index(Lang::En).starts_with(
        "⚠ --format outline output would be 2 lines — showing the method index instead."
    ));
    assert!(
        index(Lang::Hu)
            .starts_with("⚠ A --format outline kimenete 2 sor lenne — helyette metódus-index.")
    );
    let summary = |lang: Lang| limit(lang).directory_summary("a\nb".into(), "S".into(), "api", 3);
    assert!(summary(Lang::En).starts_with(
        "⚠ --format api output would be 2 lines (3 files) — showing the summary instead."
    ));
    assert!(
        summary(Lang::Hu)
            .starts_with("⚠ A --format api kimenete 2 sor lenne (3 fájl) — helyette summary.")
    );
    assert!(summary(Lang::En).ends_with("\n\nS") && summary(Lang::Hu).ends_with("\n\nS"));
}
