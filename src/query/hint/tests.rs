//! A helyreállító üzenet-generátorok: edit-távolság, javaslat-sorrend, minta-kinyerés, katalógus, és az üzenetek
//! szövege angolul és magyarul (azonos sorszámmal).

use super::*;
use crate::core::SourceFile;
use crate::lang::LanguageRegistry;

/// Csak definíciók (törzs nélkül, hívás nélkül): `User::find`, `User::save`, `Post::publish`, `helper`.
const DEFINITIONS: &str = "<?php\nnamespace App\\Models;\n\nclass User\n{\n    public static function find($id) {}\n    public function save() {}\n}\n\nclass Post\n{\n    public function publish() {}\n}\n\nfunction helper() {}\n";
/// Egyetlen hívás: a natív `strlen`.
const CALLER: &str = "<?php\nfunction caller() { return strlen('x'); }\n";

fn graph(path: &str, text: &str) -> FileGraph {
    let registry = LanguageRegistry::with_defaults();
    let language = registry.for_path(std::path::Path::new(path)).unwrap();
    language
        .analyze(&SourceFile::new(path, text.as_bytes().to_vec(), None))
        .unwrap()
        .graph
}

fn definitions() -> Catalog {
    Catalog::from_graphs([&graph("src/Models.php", DEFINITIONS)])
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn both(render: impl Fn(Lang) -> String) -> (String, String) {
    let (en, hu) = (render(Lang::En), render(Lang::Hu));
    assert_eq!(en.lines().count(), hu.lines().count(), "{en}\n{hu}");
    assert_ne!(en, hu);
    (en, hu)
}

#[test]
fn edit_distance_counts_insert_delete_and_replace() {
    assert_eq!(edit_distance("", ""), 0);
    assert_eq!(edit_distance("", "abc"), 3);
    assert_eq!(edit_distance("abc", ""), 3);
    assert_eq!(edit_distance("find", "find"), 0);
    assert_eq!(edit_distance("find", "fnd"), 1);
    assert_eq!(edit_distance("find", "finde"), 1);
    assert_eq!(edit_distance("find", "fond"), 1);
    assert_eq!(edit_distance("kitten", "sitting"), 3);
    assert_eq!(edit_distance("flaw", "lawn"), 2);
    // karakterenként számol, nem bájtonként
    assert_eq!(edit_distance("é", "e"), 1);
    assert_eq!(edit_distance("árvíz", "arviz"), 2);
    // szimmetrikus
    assert_eq!(edit_distance("sunday", "saturday"), edit_distance("saturday", "sunday"));
}

#[test]
fn closest_orders_case_then_distance_then_prefix() {
    let names = ["find", "findOrFail", "Find", "fnd", "finder", "fine", "get"];
    let found = closest_names("find", names, 10);
    // a teljesen azonos kulcs kimarad; előbb a csak betűnagyságban eltérő, majd távolság (azonos távolságnál a
    // kisebb hosszkülönbség), végül az előtag-egyezés
    assert_eq!(found, strings(&["Find", "fine", "fnd", "finder", "findOrFail"]));
    // a limit levág
    assert_eq!(closest_names("find", names, 3), strings(&["Find", "fine", "fnd"]));
    assert!(closest_names("find", names, 0).is_empty());
}

#[test]
fn closest_is_stricter_for_short_names() {
    // legfeljebb 3 karakternél a megengedett távolság 1
    assert_eq!(closest_names("id", ["ids", "ix", "name", "idle"], 5), strings(&["ix", "ids"]));
    assert!(closest_names("id", ["xyz"], 5).is_empty());
    // az előtag-egyezéshez legalább 3 karakteres rövidebb oldal kell
    assert!(closest_names("fi", ["find"], 5).is_empty());
    assert_eq!(closest_names("fin", ["finalize"], 5), strings(&["finalize"]));
    // fordítva is: a kért név hosszabb, a létező az előtagja
    assert_eq!(closest_names("getUserName", ["getUser"], 5), strings(&["getUser"]));
    assert!(closest_names("completelyDifferent", ["find", "get"], 5).is_empty());
}

#[test]
fn closest_shows_the_display_form_and_removes_duplicates() {
    let items = [("find", "User::find"), ("find", "Post::find"), ("fnd", "User::find")];
    // a kulcsot hasonlítja, a kiírt alakot adja; a kétszer szereplő kiírt alak egyszer
    assert_eq!(closest("finde", items, 5), strings(&["Post::find", "User::find"]));
}

#[test]
fn short_form_drops_namespaces_and_long_paths() {
    assert_eq!(short("App\\Models\\User::find"), "User::find");
    assert_eq!(short("crate::query::QueryEngine::new"), "QueryEngine::new");
    assert_eq!(short("this.cache.get"), "cache::get");
    assert_eq!(short("$repo->find"), "$repo::find");
    assert_eq!(short("?::find"), "find");
    assert_eq!(short("find"), "find");
    assert_eq!(short("App\\Models\\User"), "User");
    assert_eq!(short(""), "");
}

#[test]
fn plain_symbol_extracts_an_identifier_or_gives_up() {
    assert_eq!(plain_symbol("Usr::find").as_deref(), Some("Usr::find"));
    assert_eq!(plain_symbol(r"(^|::|\.|->)find$").as_deref(), Some("find"));
    assert_eq!(plain_symbol("^User::find$").as_deref(), Some("User::find"));
    assert_eq!(plain_symbol("  find ").as_deref(), Some("find"));
    assert_eq!(plain_symbol(r"Order\.refund").as_deref(), Some("Order::refund"));
    assert_eq!(plain_symbol(r"App\\Models\\User::find").as_deref(), Some("User::find"));
    assert_eq!(plain_symbol("User->find").as_deref(), Some("User::find"));
    // bármi, ami valódi reguláris kifejezés: nem azonosító
    for pattern in ["", "   ", "foo(bar|baz)", "user.*", "[a-z]+", "a|b", "x{2}", r"(^|::|\.|->)", "get?"] {
        assert_eq!(plain_symbol(pattern), None, "{pattern:?}");
    }
}

#[test]
fn name_pattern_uses_the_last_segment() {
    let by_name = r"(^|::|\.|->)find$";
    assert_eq!(name_pattern("Usr::find").as_deref(), Some(by_name));
    assert_eq!(name_pattern("find").as_deref(), Some(by_name));
    assert_eq!(name_pattern("A::B::find").as_deref(), Some(by_name));
    assert_eq!(name_pattern(""), None);
    assert_eq!(name_pattern("User::"), None);
    assert_eq!(name_pattern("a-b"), None);
    // a kinyert minta valóban megtalálja a név szerinti hívást
    let regex = crate::query::filter::pattern_regex(by_name).unwrap();
    for target in ["find", "User::find", "repo.find", "$repo->find"] {
        assert!(regex.is_match(target), "{target}");
    }
    assert!(!regex.is_match("findOrFail"));
}

#[test]
fn catalog_collects_definitions_and_suggests_in_order() {
    let catalog = definitions();
    assert!(!catalog.is_empty());
    // minősített azonosító: a teljes `Típus::metódus`-hoz hasonlít
    assert_eq!(catalog.suggest("Usr::find"), strings(&["User::find"]));
    assert_eq!(catalog.suggest("User::fnd"), strings(&["User::find"]));
    assert_eq!(catalog.suggest("user::find"), strings(&["User::find"]));
    // csak név: a metódus-/típusnévhez hasonlít, a kiírt alak `Típus::metódus`
    assert_eq!(catalog.suggest("sav"), strings(&["User::save"]));
    assert_eq!(catalog.suggest("Pst"), strings(&["Post"]));
    assert_eq!(catalog.suggest("publsh"), strings(&["Post::publish"]));
    // a létező pontos név nem javaslat önmaga
    assert!(!catalog.suggest("User::find").contains(&"User::find".to_owned()));
    assert!(catalog.suggest("zzzzzzzz").is_empty());
}

#[test]
fn catalog_limits_suggestions_to_three() {
    let mut text = String::from("<?php\nclass Repo\n{\n");
    for name in ["find1", "find2", "find3", "find4", "find5"] {
        text.push_str(&format!("    public function {name}() {{}}\n"));
    }
    text.push_str("}\n");
    let catalog = Catalog::from_graphs([&graph("src/Repo.php", &text)]);
    let found = catalog.suggest("find");
    assert_eq!(found.len(), MAX_SUGGESTIONS, "{found:?}");
    assert!(found.iter().all(|name| name.starts_with("Repo::find")));
}

#[test]
fn presence_tells_missing_from_defined_from_called() {
    let defined = definitions();
    let regex = |pattern: &str| crate::query::filter::pattern_regex(pattern).unwrap();
    assert_eq!(defined.presence(&regex("User::find")), Presence::DefinedOnly);
    assert_eq!(defined.presence(&regex(r"(^|::|\.|->)find$")), Presence::DefinedOnly);
    assert_eq!(defined.presence(&regex("^Post$")), Presence::DefinedOnly);
    assert_eq!(defined.presence(&regex("Usr::find")), Presence::Missing);
    assert_eq!(defined.presence(&regex("nothing_like_this")), Presence::Missing);
    let with_caller = Catalog::from_graphs([&graph("src/Models.php", DEFINITIONS), &graph("src/caller.php", CALLER)]);
    assert_eq!(with_caller.presence(&regex("strlen")), Presence::CalledElsewhere);
    assert_eq!(with_caller.presence(&regex("User::find")), Presence::DefinedOnly);
    // az üres katalógus semmit sem ismer
    assert_eq!(Catalog::default().presence(&regex("x")), Presence::Missing);
    assert!(Catalog::default().is_empty());
}

#[test]
fn empty_grep_explains_the_pattern_and_names_the_next_step() {
    let catalog = definitions();
    let (en, hu) = both(|lang| empty_grep(lang, "Usr::find", &catalog));
    assert!(en.starts_with("ℹ no call target matches Usr::find\n"), "{en}");
    assert!(en.contains("regular expression on the call TARGET"), "{en}");
    assert!(en.contains("(^|::|\\.|->)name$"), "{en}");
    assert!(en.contains("--guess"), "{en}");
    assert!(en.contains("Type::method finds only the resolved calls"), "{en}");
    assert!(en.contains("💡 by name only: '(^|::|\\.|->)find$'\n"), "{en}");
    assert!(en.ends_with("💡 did you mean: User::find\n"), "{en}");
    assert!(hu.starts_with("ℹ egyetlen hívás-cél sem illeszkedik a mintára: Usr::find\n"), "{hu}");
    assert!(hu.contains("reguláris kifejezés a hívás CÉLJÁRA"), "{hu}");
    assert!(hu.contains("csak név szerint: '(^|::|\\.|->)find$'"), "{hu}");
    assert!(hu.ends_with("💡 esetleg erre gondoltál: User::find\n"), "{hu}");
    // a generikus törzs-logika tipp nincs benne
    for text in [&en, &hu] {
        assert!(!text.contains("body logic") && !text.contains("törzs-logik"), "{text}");
    }
}

#[test]
fn empty_grep_skips_what_does_not_apply() {
    let catalog = definitions();
    // már név szerinti minta: nem javasolja önmagát, és a név nem hasonlít semmire
    let by_name = empty_grep(Lang::En, r"(^|::|\.|->)zork$", &catalog);
    assert_eq!(by_name.lines().count(), 2, "{by_name}");
    assert!(!by_name.contains("by name only"), "{by_name}");
    assert!(!by_name.contains("did you mean"), "{by_name}");
    // összetett reguláris kifejezés: se név, se javaslat
    let complex = empty_grep(Lang::En, "^zork(a|b)$", &catalog);
    assert_eq!(complex.lines().count(), 2, "{complex}");
    // üres katalógus: javaslat nélkül is értelmes
    let nothing = empty_grep(Lang::En, "Usr::find", &Catalog::default());
    assert!(nothing.contains("by name only"), "{nothing}");
    assert!(!nothing.contains("did you mean"), "{nothing}");
    // nem név szerinti, de egyszerű azonosító: a név szerinti minta más, ezért ajánlja
    let plain = empty_grep(Lang::En, "zork", &catalog);
    assert!(plain.contains("by name only: '(^|::|\\.|->)zork$'"), "{plain}");
}

#[test]
fn no_caller_distinguishes_missing_defined_and_called_elsewhere() {
    let catalog = definitions();
    let (en, hu) = both(|lang| no_caller(lang, "Usr::find", Presence::Missing, &catalog));
    assert_eq!(
        en,
        "  (no method named Usr::find in the project map — check the spelling)\n  💡 did you mean: User::find\n"
    );
    assert_eq!(
        hu,
        "  (nincs Usr::find nevű metódus a projekt-térképen — ellenőrizd a helyesírást)\n  💡 esetleg erre gondoltál: User::find\n"
    );
    // nincs hasonló név: csak az első sor
    let lone = no_caller(Lang::En, "zork", Presence::Missing, &catalog);
    assert_eq!(lone, "  (no method named zork in the project map — check the spelling)\n");
    // név szerinti mintából a név kerül az üzenetbe
    let named = no_caller(Lang::En, r"(^|::|\.|->)zork$", Presence::Missing, &catalog);
    assert!(named.contains("no method named zork in"), "{named}");
    // összetett minta: a minta maga
    let complex = no_caller(Lang::En, "zork(a|b)", Presence::Missing, &catalog);
    assert!(complex.contains("no method named zork(a|b) in"), "{complex}");

    let (en, hu) = both(|lang| no_caller(lang, "User::find", Presence::DefinedOnly, &catalog));
    assert_eq!(
        en,
        "  (User::find exists but nothing calls it in the project — it may be an entry point, an event/route handler or called dynamically)\n"
    );
    assert!(hu.starts_with("  (User::find létezik, de a projektben semmi sem hívja"), "{hu}");
    assert!(hu.contains("dinamikusan hívják"), "{hu}");

    let (en, hu) = both(|lang| no_caller(lang, "User::find", Presence::CalledElsewhere, &catalog));
    assert!(en.contains("is not called from this path"), "{en}");
    assert!(en.contains("search from the project root"), "{en}");
    assert!(hu.contains("a projekt más részén igen"), "{hu}");
    // minden változat egy behúzott, zárójeles sorral kezdődik (a mélységi nézet többi sora is)
    for text in [&en, &hu] {
        assert!(text.starts_with("  ("), "{text}");
    }
}

#[test]
fn did_you_mean_line_has_no_trailing_newline_and_is_empty_without_suggestions() {
    let list = strings(&["a", "b"]);
    assert_eq!(did_you_mean(Lang::En, &list), "Did you mean: a, b");
    assert_eq!(did_you_mean(Lang::Hu, &list), "Esetleg erre gondoltál: a, b");
    assert_eq!(did_you_mean(Lang::En, &[]), "");
    assert_eq!(did_you_mean(Lang::Hu, &[]), "");
}

#[test]
fn similar_paths_puts_the_same_file_name_elsewhere_first() {
    let known = strings(&[
        "app",
        "app/Http",
        "app/Http/Controllers/UserController.php",
        "app/Models",
        "app/Models/Post.php",
        "app/Models/User.php",
        "app/Services",
        "app/Services/User.php",
    ]);
    // azonos fájlnév más mappában: a hosszban legközelebbi elöl
    assert_eq!(
        similar_paths("app/Model/User.php", &known),
        strings(&["app/Models/User.php", "app/Services/User.php"])
    );
    // elgépelt fájlnév: az azonos mappa előbb, majd a távolság, majd az ábécé
    assert_eq!(
        similar_paths("app/Models/Usr.php", &known),
        strings(&["app/Models/User.php", "app/Services/User.php"])
    );
    // a kiterjesztés nélkül kért név a törzshöz hasonlít
    assert_eq!(
        similar_paths("app/Models/Usr", &known),
        strings(&["app/Models/User.php", "app/Services/User.php"])
    );
    // mappa elgépelése
    assert_eq!(similar_paths("app/Model", &known), strings(&["app/Models"]));
    // kis-/nagybetű eltérés
    assert_eq!(similar_paths("app/Models/user.php", &known).first().map(String::as_str), Some("app/Models/User.php"));
    // Windows-os elválasztó és záró perjel
    assert_eq!(similar_paths("app\\Model\\", &known), strings(&["app/Models"]));
    // nincs hasonló
    assert!(similar_paths("zzz/qqq.php", &known).is_empty());
    assert!(similar_paths("", &known).is_empty());
    // a létező útvonal nem javasolja önmagát
    assert!(!similar_paths("app/Models/User.php", &known).contains(&"app/Models/User.php".to_owned()));
}

#[test]
fn similar_paths_gives_at_most_three() {
    let known = strings(&["a/User.php", "b/User.php", "c/User.php", "d/User.php", "e/User.php"]);
    assert_eq!(similar_paths("x/User.php", &known).len(), MAX_SUGGESTIONS);
    // az azonos nevűek után a közeli nevek is elférnek, ha van hely
    let mixed = strings(&["a/User.php", "app/Uzer.php", "app/Usar.php", "app/Usor.php"]);
    let found = similar_paths("app/User.php", &mixed);
    assert_eq!(found.first().map(String::as_str), Some("a/User.php"));
    assert_eq!(found.len(), MAX_SUGGESTIONS);
}

#[test]
fn unknown_category_lists_the_known_names_and_a_close_one() {
    let known = ["external_call", "model_use", "orm_relation"];
    let (en, hu) = both(|lang| unknown_category(lang, "modle_use", &known));
    assert_eq!(
        en,
        "Unknown category: modle_use (known: external_call, model_use, orm_relation)\nDid you mean: model_use"
    );
    assert_eq!(
        hu,
        "Ismeretlen kategória: modle_use (ismertek: external_call, model_use, orm_relation)\nEsetleg erre gondoltál: model_use"
    );
    // nincs hasonló: egyetlen sor, mint a layer-hibánál
    let lone = unknown_category(Lang::En, "zzzzzz", &known);
    assert_eq!(lone, "Unknown category: zzzzzz (known: external_call, model_use, orm_relation)");
    assert_eq!(lone.lines().count(), 1);
}

#[test]
fn fixed_texts_follow_the_language_with_the_same_shape() {
    let (en, hu) = both(|lang| empty_strings(lang).to_owned());
    assert_eq!(en.lines().count(), 1);
    assert!(en.starts_with("💡 ") && hu.starts_with("💡 "));
    assert!(en.contains("case-insensitive substring"), "{en}");
    assert!(en.contains("shorter fragment"), "{en}");
    assert!(hu.contains("kis-/nagybetű-független"), "{hu}");

    let (en, hu) = both(|lang| locate_no_match(lang).to_owned());
    assert_eq!(en.lines().count(), 1);
    assert!(en.starts_with("💡 ") && hu.starts_with("💡 "));
    for needle in ["camelCase", "snake_case", "singular", "visszatérítés → refund", "--strings"] {
        assert!(en.contains(needle), "{en}");
    }
    assert!(hu.contains("visszatérítés → refund") && hu.contains("--strings"), "{hu}");
    assert!(en.contains("(\"…\")") && hu.contains("(\"…\")"));
}
