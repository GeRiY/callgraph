use std::collections::HashSet;

use super::{Comp, Kind, Project, Prop, camelize, falls_through, pascal};
use crate::lang::js::{Sfc, template_facts};

/// A component with the given props (`(name, required)`) and template; the import of `import_of` is
/// `alias -> display`.
fn comp(rel: &str, props: &[(&str, bool)], template: &str, imports: &[(&str, &str)]) -> Comp {
    let text = format!("<template>\n{template}\n</template>\n");
    let facts = template_facts(&text, &Sfc::parse(&text)).unwrap_or_default();
    let stem = rel
        .rsplit('/')
        .next()
        .and_then(|file| file.strip_suffix(".vue"))
        .unwrap_or(rel)
        .to_string();
    Comp {
        rel: rel.to_string(),
        stem,
        props: props
            .iter()
            .map(|(name, required)| Prop {
                name: camelize(name),
                required: *required,
            })
            .collect(),
        models: Vec::new(),
        props_complete: true,
        slots: None,
        facts,
        imports: imports
            .iter()
            .map(|(alias, display)| ((*alias).to_string(), (*display).to_string()))
            .collect(),
    }
}

fn check(comps: Vec<Comp>, auto: bool, parent: &str) -> Vec<(Kind, String)> {
    let project = Project { comps, auto };
    let scope: HashSet<String> = HashSet::from([parent.to_string()]);
    project
        .check(&scope)
        .findings
        .into_iter()
        .map(|f| (f.kind, f.name))
        .collect()
}

const CARD: &str = "src/Card.vue";
const IMPORT: [(&str, &str); 1] = [("Card", "src/Card.vue")];

#[test]
fn missing_required_and_unknown_props_are_found() {
    let card = comp(CARD, &[("title", true), ("size", false)], "<div/>", &[]);
    let page = comp(
        "src/Page.vue",
        &[],
        "<Card colr=\"red\" size=\"s\" />",
        &IMPORT,
    );
    let found = check(vec![card, page], false, "src/Page.vue");
    assert_eq!(
        found,
        [
            (Kind::MissingProp, "title".to_string()),
            (Kind::UnknownProp, "colr".to_string())
        ]
    );
}

#[test]
fn kebab_and_camel_case_are_the_same_name() {
    let card = comp(CARD, &[("cellId", true)], "<div/>", &[]);
    let page = comp(
        "src/Page.vue",
        &[],
        "<Card :cell-id=\"1\" /><Card cellId=\"2\" />",
        &IMPORT,
    );
    assert!(check(vec![card, page], false, "src/Page.vue").is_empty());
}

#[test]
fn a_spread_or_an_unknown_prop_set_is_never_a_finding() {
    let mut card = comp(CARD, &[("title", true)], "<div/>", &[]);
    let page = comp(
        "src/Page.vue",
        &[],
        "<Card v-bind=\"bag\" zzz=\"1\" />",
        &IMPORT,
    );
    // `v-bind="obj"`: the required prop may be in it, but an unknown attribute is still unknown
    let found = check(vec![card, page], false, "src/Page.vue");
    assert_eq!(found, [(Kind::UnknownProp, "zzz".to_string())]);
    // `$attrs` / imported / composed props: attributes are not decidable
    card = comp(CARD, &[("title", true)], "<div/>", &[]);
    card.props_complete = false;
    let page = comp(
        "src/Page.vue",
        &[],
        "<Card title=\"t\" zzz=\"1\" />",
        &IMPORT,
    );
    assert!(check(vec![card, page], false, "src/Page.vue").is_empty());
}

#[test]
fn v_model_is_a_prop_and_html_attributes_fall_through() {
    let mut card = comp(CARD, &[("modelValue", true)], "<div/>", &[]);
    card.models = vec!["open".to_string()];
    let page = comp(
        "src/Page.vue",
        &[],
        "<Card v-model=\"a\" v-model:open=\"b\" id=\"x\" class=\"c\" data-k=\"1\" aria-label=\"l\" @click=\"go\" />",
        &IMPORT,
    );
    assert!(check(vec![card, page], false, "src/Page.vue").is_empty());
}

#[test]
fn parent_slots_must_exist_when_the_child_slots_are_known() {
    let mut card = comp(CARD, &[], "<div><slot name=\"head\" /></div>", &[]);
    card.slots = Some(vec!["head".to_string(), "default".to_string()]);
    let page = comp(
        "src/Page.vue",
        &[],
        "<Card><template #head>h</template><template v-slot:foot>f</template></Card>",
        &IMPORT,
    );
    let found = check(vec![card, page], false, "src/Page.vue");
    assert_eq!(found, [(Kind::UnknownSlot, "foot".to_string())]);
    // not statically known: no finding
    let card = comp(CARD, &[], "<div/>", &[]);
    let page = comp(
        "src/Page.vue",
        &[],
        "<Card><template #foot>f</template></Card>",
        &IMPORT,
    );
    assert!(check(vec![card, page], false, "src/Page.vue").is_empty());
}

#[test]
fn auto_imports_bind_only_with_the_plugin_and_one_file() {
    let card = || comp(CARD, &[("title", true)], "<div/>", &[]);
    let page = || comp("src/Page.vue", &[], "<Card />", &[]);
    assert!(check(vec![card(), page()], false, "src/Page.vue").is_empty());
    assert_eq!(
        check(vec![card(), page()], true, "src/Page.vue"),
        [(Kind::MissingProp, "title".to_string())]
    );
    // two files with that name: ambiguous, no binding
    let other = comp("lib/Card.vue", &[("title", true)], "<div/>", &[]);
    assert!(check(vec![card(), other, page()], true, "src/Page.vue").is_empty());
}

#[test]
fn an_import_binds_through_the_displayed_path_when_it_is_unique() {
    let card = comp("ui/src/kit/Card.vue", &[("title", true)], "<div/>", &[]);
    let other = comp("ui/src/old/Card.vue", &[("title", true)], "<div/>", &[]);
    let page =
        |display: &'static str| comp("ui/src/Page.vue", &[], "<Card />", &[("Card", display)]);
    let one = check(
        vec![card, other, page("src/kit/Card.vue")],
        false,
        "ui/src/Page.vue",
    );
    assert_eq!(one, [(Kind::MissingProp, "title".to_string())]);
    // an alias keeps only the tail: `Card.vue` alone is ambiguous
    let card = comp("ui/src/kit/Card.vue", &[("title", true)], "<div/>", &[]);
    let other = comp("ui/src/old/Card.vue", &[("title", true)], "<div/>", &[]);
    assert!(
        check(
            vec![card, other, page("@kit/Card.vue")],
            false,
            "ui/src/Page.vue"
        )
        .is_empty()
    );
}

#[test]
fn names_are_normalised() {
    assert_eq!(camelize("some-prop"), "someProp");
    assert_eq!(camelize("update:model-value"), "update:modelValue");
    assert_eq!(pascal("message-actions"), "MessageActions");
    assert_eq!(pascal("MessageActions"), "MessageActions");
    assert!(falls_through("aria-label") && falls_through("data-x") && falls_through("onClick"));
    assert!(!falls_through("colr"));
}
