//! A fájltípusok közti kapcsolatok tesztjei: a PHP nyelvi tömb, a fordítási hívások olvasása, a nyelvi halmazok
//! (hiányzó / nem használt / ismeretlen kulcs, dinamikus előtag), a CSS-osztályok definíciói és használatai
//! (engedékeny „nem használt”), és egy lemezen épített mini-projekt nézetei.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::classes::Classes;
use super::classes_scan::{Uses, definitions, scan_uses};
use super::i18n::{I18n, KeyState, lang_target};
use super::i18n_scan::{Found, php_leaves, scan};
use super::{CrossRef, I18nNeed, Need, Scope, need_for};
use crate::i18n::Lang;
use crate::model::FileGraphBuilder;
use crate::query::QueryResult;

fn scope(rel: &str, names: Option<&[&str]>) -> Scope {
    Scope {
        rel: rel.to_owned(),
        names: names.map(|names| names.iter().map(|name| (*name).to_owned()).collect()),
    }
}

#[test]
fn php_lang_array_leaves_with_nesting_comments_and_expressions() {
    let text = "<?php\n// c\nreturn [\n    'title' => 'Hello',\n    'nested' => [\n        'a' => 'A',\n        'b' => ['c' => 'C'],\n    ],\n    'dyn' => 'x' . $y,\n    'fn' => __('other'),\n    'last',\n];\n";
    let leaves: Vec<(String, Option<String>)> = php_leaves(text, "messages")
        .into_iter()
        .map(|(path, _, value)| (path, value))
        .collect();
    let want: Vec<(&str, Option<&str>)> = vec![
        ("messages.title", Some("Hello")),
        ("messages.nested.a", Some("A")),
        ("messages.nested.b.c", Some("C")),
        ("messages.dyn", None),
        ("messages.fn", None),
        ("messages.0", Some("last")),
    ];
    let got: Vec<(&str, Option<&str>)> = leaves
        .iter()
        .map(|(path, value)| (path.as_str(), value.as_deref()))
        .collect();
    assert_eq!(got, want);
    assert_eq!(php_leaves(text, "m")[0].1, 4);
}

#[test]
fn lang_files_are_recognized_by_path() {
    let target = |rel: &str| lang_target(rel);
    assert_eq!(target("lang/hu.json"), Some(("lang".into(), "hu".into())));
    assert_eq!(
        target("resources/js/lang/en.json"),
        Some(("resources/js/lang".into(), "en".into()))
    );
    assert_eq!(
        target("lang/en/auth.php"),
        Some(("lang".into(), "en".into()))
    );
    assert_eq!(
        target("resources/lang/pt-BR/x.php"),
        Some(("resources/lang".into(), "pt-BR".into()))
    );
    assert_eq!(
        target("locales/en/common.json"),
        Some(("locales".into(), "en".into()))
    );
    for other in [
        "lang/auth.json",
        "config/app.json",
        "package.json",
        "lang/en/x.blade.php",
        "app/Models/User.php",
    ] {
        assert_eq!(target(other), None, "{other}");
    }
}

#[test]
fn translation_calls_are_read_and_dynamic_keys_only_give_prefixes() {
    let mut found = Found::default();
    scan(
        "a.js",
        "js",
        "t('a.b'); this.$t(\"c.d\"); x.t('no'); format('z'); $t('with space'); t('p.' + k); i18n.t('i.j'); const k = t(name);",
        &mut found,
    );
    let keys: Vec<&str> = found.usages.iter().map(|u| u.key.as_str()).collect();
    assert_eq!(keys, ["a.b", "c.d", "i.j"]);
    assert_eq!(found.prefixes, ["p."]);
    assert_eq!(found.variable_args, 1);
    assert!(found.literals.contains("a.b") && !found.literals.contains("no"));

    let mut php = Found::default();
    scan(
        "a.php",
        "php",
        "<?php echo __('Hi there'); echo trans(\"m.$x\"); echo trans('m.' . $k); @lang('v.w'); $this->trans('no'); Lang::get('l.k'); t('no');",
        &mut php,
    );
    let keys: Vec<&str> = php.usages.iter().map(|u| u.key.as_str()).collect();
    assert_eq!(keys, ["Hi there", "v.w", "l.k"]);
    assert_eq!(php.prefixes, ["m.", "m."]);

    let mut template = Found::default();
    scan(
        "b.vue",
        "vue",
        "<i18n-t keypath=\"k.one\"/><p v-t=\"'k.two'\"></p>\n<p>{{ $t(`dyn.${x}`) }}</p>",
        &mut template,
    );
    let keys: Vec<&str> = template.usages.iter().map(|u| u.key.as_str()).collect();
    assert_eq!(keys, ["k.one", "k.two"]);
    assert_eq!(template.prefixes, ["dyn."]);
}

fn sample_i18n() -> I18n {
    let mut i18n = I18n::default();
    i18n.add_lang(
        "lang/en.json",
        r#"{"board":{"title":"Title","add":"Add"},"old":"Old","Welcome":"Welcome"}"#,
    );
    i18n.add_lang("lang/hu.json", r#"{"board":{"title":"Cím"},"only_hu":"x"}"#);
    i18n.add_source(
        "resources/js/Board.vue",
        "vue",
        "<template>{{ $t('board.title') }} {{ $t('board.add') }} {{ $t('nope.key') }} {{ $t(`dyn.${x}`) }}</template>",
    );
    i18n.add_source(
        "app/C.php",
        "php",
        "<?php echo __('Welcome'); echo __('board.title');",
    );
    i18n.finish();
    i18n
}

#[test]
fn key_states_follow_the_lang_files() {
    let i18n = sample_i18n();
    assert_eq!(i18n.check("board.title"), KeyState::Ok);
    assert_eq!(i18n.check("board"), KeyState::Ok);
    assert_eq!(
        i18n.check("board.add"),
        KeyState::Missing(vec!["hu.json".into()])
    );
    assert_eq!(i18n.check("nope.key"), KeyState::Unknown);
    assert_eq!(i18n.check("pkg::file.key"), KeyState::Skip);
    assert_eq!(i18n.check("validation.required"), KeyState::Skip);
    assert_eq!(
        i18n.warning("board.add", Lang::En).as_deref(),
        Some(" ⚠ missing in hu.json")
    );
    assert_eq!(
        i18n.warning("nope.key", Lang::En).as_deref(),
        Some(" ⚠ no key")
    );
    assert_eq!(i18n.warning("board.title", Lang::En), None);
    assert_eq!(i18n.found.prefixes, ["dyn."]);
    assert_eq!(i18n.found.usages.len(), 5);
    // nincs nyelvfájl: semmi nem ellenőrizhető
    assert_eq!(I18n::default().check("x.y"), KeyState::Skip);
}

#[test]
fn file_reports_list_missing_lacking_and_unused_keys() {
    let i18n = sample_i18n();
    let en = i18n.file_report("lang/en.json", 5).expect("en report");
    assert_eq!(
        en.missing_in,
        [(
            "hu".to_owned(),
            3,
            vec![
                "Welcome".to_owned(),
                "board.add".to_owned(),
                "old".to_owned()
            ]
        )]
    );
    assert_eq!(en.lacks, [("hu".to_owned(), 1, vec!["only_hu".to_owned()])]);
    assert_eq!(en.unused, (1, vec!["old".to_owned()]));
    let hu = i18n.file_report("lang/hu.json", 5).expect("hu report");
    assert_eq!(hu.missing_in.len(), 1);
    assert_eq!(hu.lacks[0].1, 3);
    assert_eq!(hu.unused, (1, vec!["only_hu".to_owned()]));
    assert!(i18n.file_report("resources/js/Board.vue", 5).is_none());
    let rows = i18n.key_rows("lang/en.json", "board", 8);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].key, "board.add");
    assert!(rows[0].values[1].1.is_none());
}

#[test]
fn sentence_keys_are_never_unused_and_long_keys_print_on_one_line() {
    let long = "If you're having trouble\nclicking the \":actionText\" button, copy the URL below";
    let json = serde_json::json!({
        "Hello!": "x", "Regards,": "x", "Dear :name": "x", "plain": "x", (long): "x"
    });
    let mut i18n = I18n::default();
    i18n.add_lang("lang/en.json", &json.to_string());
    i18n.add_lang("lang/hu.json", r#"{"plain":"y"}"#);
    i18n.add_source("tests/Feature/KeyTest.php", "php", "<?php $k = 'a.b';");
    i18n.finish();
    let report = i18n.file_report("lang/en.json", 9).expect("report");
    assert_eq!(report.unused, (1, vec!["plain".to_owned()]));
    // a mondat-kulcs a másik nyelvből továbbra is hiányzik
    assert_eq!(report.missing_in[0].1, 4);
    let cr = CrossRef {
        i18n: Some(i18n),
        ..Default::default()
    };
    let lines = cr.i18n_view(&[scope("lang/en.json", None)], Lang::En);
    assert!(lines.iter().all(|line| !line.contains('\n')), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|line| line.contains("If you're having trouble clicking the \":…"))
    );
}

#[test]
fn quoted_references_and_parent_objects_count_as_used() {
    let mut i18n = I18n::default();
    i18n.add_lang(
        "lang/en.json",
        r#"{"menu":{"home":"Home","about":"About"},"nav":{"x":"X"},"lonely":"L"}"#,
    );
    i18n.add_source(
        "a.js",
        "js",
        "const m = $t('menu'); const items = [{ label: 'nav.x' }];",
    );
    i18n.finish();
    let report = i18n.file_report("lang/en.json", 5).expect("report");
    assert_eq!(report.unused, (1, vec!["lonely".to_owned()]));
}

#[test]
fn css_definitions_follow_simple_class_selectors() {
    let css = ".btn { color: red; }\n.btn-primary, .card > .title:hover { }\n@media (min-width: 10px) { .in-media { } }\n@keyframes spin { from { } to { } }\n.a:not(.b) { }\n.md\\:flex { }\n/* .commented { } */\n.parent { &-child { } &.active { } .nested { } }\n@mixin m { .in-mixin { } }\n";
    let mut names: Vec<(String, u32)> = definitions(css);
    names.sort();
    names.dedup();
    let plain: Vec<&str> = names.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        plain,
        [
            "a",
            "active",
            "btn",
            "btn-primary",
            "card",
            "in-media",
            "nested",
            "parent",
            "parent-child",
            "title"
        ]
    );
    assert!(names.contains(&("btn".to_owned(), 1)));
}

#[test]
fn class_uses_come_from_attributes_literals_and_modules_with_dynamic_parts() {
    let defined: HashSet<String> = ["btn", "card", "active", "is-open", "cls-a"]
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let mut uses = Uses::default();
    scan_uses(
        "<div class=\"btn card\" :class=\"{ active: on, 'is-open': open }\"></div>\n<script>const size = 'btn-' + s; const c = `hero-${x}`; el.classList.add('card'); styles.clsA; styles['cls-a'];</script>",
        0,
        &defined,
        &mut uses,
    );
    for class in ["btn", "card", "active", "is-open", "cls-a"] {
        assert!(uses.words.contains_key(class), "{class}");
    }
    assert!(uses.words["card"].contains(&(0, 2)));
    assert!(uses.prefixes.contains("btn-") && uses.prefixes.contains("hero-"));
}

#[test]
fn unused_classes_are_reported_leniently() {
    let mut classes = Classes::default();
    classes.add_stylesheet(
        "resources/css/app.css",
        ".btn{} .btn-primary{} .never-used{} .size-sm{} .hero-big{}\n",
    );
    let defined = classes.defined();
    classes.add_source(
        "A.vue",
        "<div class=\"btn\"></div><script>const k = 'size-' + n; const h = `x-${y}-big`;</script>",
        &defined,
    );
    classes.finish();
    assert!(!classes.is_unused("btn"));
    assert!(classes.is_unused("btn-primary"));
    assert!(classes.is_unused("never-used"));
    assert!(!classes.is_unused("size-sm"));
    assert_eq!(classes.dynamic_hint("size-sm").as_deref(), Some("size-…"));
    assert_eq!(classes.used_by("btn"), [("A.vue", 1)]);
    let unused: Vec<String> = classes
        .unused_in("resources/css/app.css")
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(unused, ["btn-primary", "never-used"]);
}

#[test]
fn tailwind_utilities_and_vendor_stylesheets_are_skipped() {
    let mut classes = Classes::default();
    classes.add_stylesheet(
        "a.css",
        ".text-primary{} .card-x{}\n.y{ @apply p-4 card-x; }\n",
    );
    classes.add_stylesheet("vendor.min.css", ".never{}");
    classes.add_stylesheet("b.css", "/*! Bootstrap v5.3 */\n.col-md-6{}");
    classes.finish();
    assert!(classes.tailwind);
    let unused: Vec<String> = classes
        .unused_in("a.css")
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    // a `card-x` az `@apply`-ban használt, a `text-primary` utility
    assert_eq!(unused, ["y"]);
    assert!(!classes.defs.contains_key("never"));
    assert!(!classes.defs.contains_key("col-md-6"));
}

fn project(files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "cg-crossref-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }
    dir
}

const EN_JSON: &str = "{\n  \"board\": {\n    \"title\": \"Title\",\n    \"add\": \"Add\"\n  },\n  \"old\": \"Old\"\n}\n";
const HU_JSON: &str = "{\n  \"board\": {\n    \"title\": \"Cím\"\n  }\n}\n";
const BOARD_VUE: &str = "<template>\n  <h1 :class=\"{ active: a }\">{{ $t('board.title') }}</h1>\n  <p class=\"btn\">{{ $t('board.nope') }}</p>\n</template>\n";
const APP_CSS: &str = ".btn { color: red; }\n.active { color: blue; }\n.dead { color: gray; }\n";

#[test]
fn views_and_headers_on_a_mini_project() {
    let root = project(&[
        ("lang/en.json", EN_JSON),
        ("lang/hu.json", HU_JSON),
        ("resources/js/Board.vue", BOARD_VUE),
        ("resources/css/app.css", APP_CSS),
    ]);
    let need = Need {
        i18n: I18nNeed::Full,
        classes: true,
    };
    let crossref = CrossRef::open(&root, need);

    let file = crossref.i18n_view(&[scope("lang/en.json", None)], Lang::En);
    assert_eq!(
        file,
        [
            "i18n lang: en, hu · 3 keys",
            "lang/en.json  missing in hu 2: board.add, old · unused 2: board.add, old"
        ]
    );
    let source = crossref.i18n_view(&[scope("resources/js/Board.vue", None)], Lang::En);
    assert_eq!(
        source,
        [
            "i18n lang: en, hu · 3 keys",
            "⚠ board.nope · no key ← Board.vue:3"
        ]
    );
    let key = crossref.i18n_view(&[scope("lang/en.json", Some(&["board.title"]))], Lang::En);
    assert_eq!(
        key[1],
        "board.title  en.json:3 \"Title\" · hu.json:3 \"Cím\" · used: Board.vue:2"
    );

    assert_eq!(
        crossref.header("lang/en.json", Lang::En).as_deref(),
        Some("i18n: missing in hu: 2 · unused: 2")
    );
    assert_eq!(
        crossref.header("lang/hu.json", Lang::En).as_deref(),
        Some("i18n: lacks 2 vs en")
    );
    assert_eq!(
        crossref
            .header("resources/css/app.css", Lang::En)
            .as_deref(),
        Some("classes: unused .dead")
    );
    assert_eq!(crossref.header("resources/js/Board.vue", Lang::En), None);

    let sheet = crossref.classes_view(&[scope("resources/css/app.css", None)], Lang::En);
    assert_eq!(
        sheet,
        [
            "classes: 3 defined · 1 unused",
            "resources/css/app.css  unused 1: .dead L3"
        ]
    );
    let rule = crossref.classes_view(&[scope("resources/css/app.css", Some(&[".btn"]))], Lang::En);
    assert_eq!(rule[1], ".btn  app.css:1 · used by: Board.vue:3");
    let styles = crossref.classes_view(&[scope("resources/js/Board.vue", None)], Lang::En);
    assert_eq!(
        styles,
        ["resources/js/Board.vue  styles: app.css .active, .btn"]
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn nothing_is_read_for_unrelated_files_and_nothing_is_said_without_data() {
    let row = |file: &str| QueryResult::new(FileGraphBuilder::new(file).build(), None);
    assert_eq!(
        need_for(&[row("app/x.php")], "outline").i18n,
        I18nNeed::None
    );
    assert!(!need_for(&[row("app/x.php")], "outline").classes);
    assert_eq!(
        need_for(&[row("lang/en.json")], "outline").i18n,
        I18nNeed::Full
    );
    assert!(need_for(&[row("a.css")], "outline").classes);
    assert_eq!(need_for(&[row("x.php")], "i18n").i18n, I18nNeed::Full);
    assert!(need_for(&[row("x.php")], "classes").classes);
    let empty = CrossRef::default();
    assert!(empty.i18n_view(&[], Lang::En).is_empty());
    assert!(empty.classes_view(&[], Lang::En).is_empty());
    assert_eq!(empty.header("lang/en.json", Lang::En), None);
}
