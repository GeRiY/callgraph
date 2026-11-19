//! Template expressions through the script analyser (`script::ExprAnalyzer`).

use crate::core::SourceFile;
use crate::lang::{Language, js::JsFrontend};
use crate::model::{Call, DYNAMIC_CALL, DYNAMIC_KIND};

/// The `caller: "template"` calls of a `Box.vue` file.
fn template_calls(src: &str) -> Vec<Call> {
    let a = JsFrontend::with_defaults()
        .analyze(&SourceFile::new(
            "/p/Box.vue",
            src.as_bytes().to_vec(),
            None,
        ))
        .unwrap();
    a.graph
        .calls
        .into_iter()
        .filter(|c| c.caller.as_deref() == Some("template"))
        .collect()
}

fn named<'a>(calls: &'a [Call], target: &str) -> Vec<&'a Call> {
    calls.iter().filter(|c| c.target == target).collect()
}

fn category_of(calls: &[Call], target: &str) -> Option<String> {
    named(calls, target).first().map(|c| c.category.clone())
}

#[test]
fn v_for_locals_are_not_calls_but_the_rest_is_classified() {
    let c = template_calls(concat!(
        "<template>\n  <ul>\n",
        "    <li v-for=\"(item, i) in rows()\" :key=\"i\" @click=\"item.open(); pick(item)\">{{ fmt(item) }} {{ other() }}</li>\n",
        "  </ul>\n</template>\n",
        "<script setup>\nfunction rows() { return [] }\nfunction pick(x) {}\nfunction fmt(x) {}\n</script>\n",
    ));
    for name in ["rows", "pick", "fmt"] {
        assert_eq!(
            category_of(&c, name).as_deref(),
            Some("internal_helper"),
            "{name}: {c:?}"
        );
    }
    assert_eq!(
        category_of(&c, "other").as_deref(),
        Some("global_helper"),
        "{c:?}"
    );
    assert!(
        !c.iter()
            .any(|x| x.target.contains("open") || x.on.as_deref() == Some("item")),
        "{c:?}"
    );
}

#[test]
fn slot_props_are_local_inside_the_slot_only() {
    let c = template_calls(concat!(
        "<template>\n",
        "  <Card #cell=\"{ a, b }\">{{ a(b) }}{{ later() }}</Card>\n",
        "  <p>{{ a() }}</p>\n",
        "</template>\n",
    ));
    let a_calls = named(&c, "a");
    assert_eq!(a_calls.len(), 1, "{c:?}");
    assert_eq!(a_calls[0].line, 3, "{c:?}");
    assert_eq!(category_of(&c, "later").as_deref(), Some("global_helper"));
}

#[test]
fn a_handler_name_is_a_reference_and_a_handler_call_is_a_call() {
    let c = template_calls(concat!(
        "<template><button @click=\"save\">a</button><button @click=\"save()\">b</button></template>\n",
        "<script setup>\nfunction save() {}\n</script>\n",
    ));
    let saves = named(&c, "save");
    assert_eq!(saves.len(), 2, "{c:?}");
    assert!(saves.iter().all(|s| s.category == "internal_helper"));
    assert_eq!(saves.iter().filter(|s| s.handler == Some(true)).count(), 1);
}

#[test]
fn inline_statement_handlers_are_walked() {
    let c = template_calls(concat!(
        "<template><button @click=\"save(); count++; if (ok) done()\">a</button></template>\n",
        "<script setup>\nfunction save() {}\nfunction done() {}\n</script>\n",
    ));
    assert_eq!(
        category_of(&c, "save").as_deref(),
        Some("internal_helper"),
        "{c:?}"
    );
    assert_eq!(
        category_of(&c, "done").as_deref(),
        Some("internal_helper"),
        "{c:?}"
    );
}

#[test]
fn emit_i18n_and_route_helpers_keep_their_keys() {
    let c = template_calls(concat!(
        "<template><button @click=\"$emit('close')\">{{ $t('menu.open') }}</button><a :href=\"route('home')\" /></template>\n",
        "<script>\nexport default { methods: {} }\n</script>\n",
    ));
    let key = |category: &str| {
        c.iter()
            .find(|x| x.category == category)
            .and_then(|x| x.resolved.clone())
    };
    assert_eq!(
        key("vue_emit").as_deref(),
        Some("Box.vue::emit:close"),
        "{c:?}"
    );
    assert_eq!(
        key("i18n_helper").as_deref(),
        Some("i18n::menu.open"),
        "{c:?}"
    );
    assert_eq!(key("route_helper").as_deref(), Some("route::home"), "{c:?}");
}

#[test]
fn script_bindings_classify_template_calls() {
    let c = template_calls(concat!(
        "<template><p>{{ debounce(q) }} {{ store.send() }} {{ Math.max(1, 2) }}</p></template>\n",
        "<script setup>\nimport { debounce } from 'lodash'\nimport { useChatStore } from './chat'\nconst store = useChatStore()\n</script>\n",
    ));
    assert_eq!(
        category_of(&c, "debounce").as_deref(),
        Some("package_use"),
        "{c:?}"
    );
    let send = c.iter().find(|x| x.target.ends_with("send")).unwrap();
    assert!(
        send.resolved
            .as_deref()
            .is_some_and(|r| r.ends_with("useChatStore().send")),
        "{c:?}"
    );
    assert_eq!(
        category_of(&c, "Math.max").as_deref(),
        Some("native_js"),
        "{c:?}"
    );
}

#[test]
fn an_unparsable_expression_falls_back_to_the_regex() {
    let c = template_calls("<template><p>{{ go( + }}</p></template>\n");
    assert_eq!(
        category_of(&c, "go").as_deref(),
        Some("external_call"),
        "{c:?}"
    );
}

#[test]
fn a_computed_key_call_is_marked_dynamic() {
    let c = template_calls(
        "<template><button @click=\"handlers[name]()\">a</button></template>\n<script setup>\nconst handlers = {}\n</script>\n",
    );
    assert!(
        c.iter()
            .any(|x| x.kind == DYNAMIC_KIND && x.category == DYNAMIC_CALL),
        "{c:?}"
    );
}

#[test]
fn a_template_literal_i18n_key_keeps_its_text() {
    let c = template_calls(
        "<template><p>{{ $t(`auth.${err}`) }} {{ $t('a.b') }}</p></template>\n<script setup>\nconst err = 1\n</script>\n",
    );
    let keys: Vec<_> = c
        .iter()
        .filter(|x| x.category == "i18n_helper")
        .filter_map(|x| x.resolved.as_deref())
        .collect();
    assert_eq!(keys, ["i18n::auth.${err}", "i18n::a.b"], "{c:?}");
}

#[test]
fn value_methods_on_unwrapped_refs_are_not_vue_api() {
    let c = template_calls(concat!(
        "<template><p>{{ name.trim() }} {{ props.ids.includes(1) }} {{ total.toFixed() }}</p></template>\n",
        "<script setup>\nimport { ref, computed } from 'vue'\n",
        "const name = ref('')\nconst total = computed(() => 1)\nconst props = defineProps(['ids'])\n</script>\n",
    ));
    for t in ["trim", "includes", "toFixed"] {
        let call = named(&c, t);
        assert_eq!(call.len(), 1, "{c:?}");
        assert_eq!(
            (call[0].category.as_str(), call[0].resolved.as_deref()),
            ("external_call", None),
            "{c:?}"
        );
    }
}
