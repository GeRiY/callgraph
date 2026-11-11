//! A dinamikus (`dynamic_call`) hívás-jelölés tesztjei JS / TS / Vue forráson: a pozitív minták, a literál
//! (nem dinamikus) esetek és a közönséges hívások változatlansága.

use crate::core::SourceFile;
use crate::lang::{Language, js::JsFrontend};
use crate::model::Call;

fn analyze(path: &str, src: &str) -> Vec<Call> {
    JsFrontend::with_defaults()
        .analyze(&SourceFile::new(path, src.as_bytes().to_vec(), None))
        .unwrap()
        .graph
        .calls
}

/// A `dynamic_call` jelölések céljai (egy TS fájl `function f(obj, k, p) { <body> }` törzsében).
fn dynamic(body: &str) -> Vec<String> {
    let src = format!("function f(obj: any, k: string, p: string) {{\n{body}\n}}\n");
    analyze("/p/x.ts", &src)
        .into_iter()
        .filter(|c| c.category == "dynamic_call")
        .map(|c| c.target)
        .collect()
}

fn none(body: &str) {
    assert_eq!(dynamic(body), Vec::<String>::new(), "{body}");
}

#[test]
fn computed_member_call_with_non_literal_key() {
    assert_eq!(dynamic("obj[k]();"), vec!["obj[k]"]);
    assert_eq!(dynamic("obj[action](1, 2);"), vec!["obj[action]"]);
    assert_eq!(dynamic("this.handlers[k](1);"), vec!["this.handlers[k]"]);
    assert_eq!(dynamic("obj['a' + k]();"), vec!["obj['a' + k]"]);
    assert_eq!(dynamic("obj[`on${k}`]();"), vec!["obj[`on${k}`]"]);
}

#[test]
fn computed_member_call_variants() {
    assert_eq!(dynamic("obj?.[k]();"), vec!["obj?.[k]"]);
    assert_eq!(dynamic("(obj as any)[k]();"), vec!["(obj as any)[k]"]);
    assert_eq!(dynamic("obj![k]();"), vec!["obj![k]"]);
}

#[test]
fn dynamic_member_call_is_one_marker_and_no_bogus_edge() {
    let all = analyze("/p/x.ts", "function f(obj, k) { obj[k](); }\n");
    assert_eq!(all.len(), 1, "{all:?}");
    let c = &all[0];
    assert_eq!(
        (c.kind.as_str(), c.category.as_str(), c.target.as_str()),
        ("dynamic", "dynamic_call", "obj[k]")
    );
    assert!(c.resolved.is_none() && c.on.is_none());
    assert_eq!(c.caller.as_deref(), Some("f"));
    assert_eq!(c.line, 1);
}

#[test]
fn literal_key_member_call_is_not_dynamic() {
    none("obj['x']();");
    none("obj[\"x\"]();");
    none("obj[0]();");
    none("obj[`x`]();");
}

#[test]
fn new_with_expression_callee() {
    assert_eq!(dynamic("new (getClass())(1);"), vec!["new (getClass())"]);
    assert_eq!(dynamic("new (a || b)();"), vec!["new (a || b)"]);
    assert_eq!(dynamic("new obj[k]();"), vec!["new obj[k]"]);
    assert_eq!(dynamic("new (obj[k])(1);"), vec!["new (obj[k])"]);
}

#[test]
fn ordinary_new_is_not_dynamic() {
    none("new Foo(1);");
    none("new ns.Foo(1);");
    none("new (Foo)(1);");
    none("new (class {})();");
    none("new Map();");
    none("new obj['Foo']();");
    none("new this.ctor();");
}

#[test]
fn dynamic_import_with_non_literal_source() {
    assert_eq!(dynamic("import(p);"), vec!["import(p)"]);
    assert_eq!(
        dynamic("import(`./locale/${k}.js`);"),
        vec!["import(`./locale/${k}.js`)"]
    );
    assert_eq!(dynamic("import('./a/' + p);"), vec!["import('./a/' + p)"]);
}

#[test]
fn literal_import_is_not_dynamic() {
    none("import('./a.js');");
    none("import(`./a.js`);");
}

#[test]
fn literal_import_keeps_its_old_record() {
    let all = analyze("/p/x.ts", "function f() { import('./a.js'); }\n");
    assert!(
        all.iter()
            .any(|c| c.target == "<expr>" && c.category == "external_call"),
        "{all:?}"
    );
}

#[test]
fn require_with_non_literal_argument() {
    assert_eq!(dynamic("require(p);"), vec!["require(p)"]);
    assert_eq!(dynamic("require('./x' + k);"), vec!["require('./x' + k)"]);
    // a `require` hívás marad, mellé jön a jelölés
    let all = analyze("/p/x.js", "function f(p) { require(p); }\n");
    assert!(all.iter().any(|c| c.target == "require" && c.category == "native_js"));
    assert!(all.iter().any(|c| c.category == "dynamic_call"));
}

#[test]
fn require_with_literal_argument_is_not_dynamic() {
    none("require('fs');");
    none("require(`fs`);");
    none("require();");
}

#[test]
fn ordinary_calls_are_unchanged() {
    none("fn(); a.b(); a?.b(); a.b.c(); fn.call(this, 1); fn.apply(null, []); obj.m?.(); new Foo();");
    none("[1, 2].map((x) => x * 2);");
    none("(() => 1)();");
    none("(0, obj.m)(1);");
    none("const o = { a: 1 }; o.a;");
    let all = analyze("/p/x.ts", "function f(a) { g(); a.b(); a?.b(); }\n");
    let cats: Vec<_> = all.iter().map(|c| c.category.as_str()).collect();
    assert!(!cats.contains(&"dynamic_call"), "{cats:?}");
}

#[test]
fn long_expression_is_shortened() {
    let long = format!("obj[{}]", "k + ".repeat(30) + "k");
    let found = dynamic(&format!("{long}();"));
    assert_eq!(found.len(), 1);
    assert!(found[0].chars().count() <= 60, "{}", found[0]);
    assert!(found[0].ends_with('…'));
}

#[test]
fn works_in_methods_and_at_module_level() {
    let all = analyze(
        "/p/x.ts",
        "class A {\n  run(k: string) {\n    this.m[k]();\n  }\n}\nconst h: any = {};\nh[name]();\n",
    );
    let dyns: Vec<_> = all.iter().filter(|c| c.category == "dynamic_call").collect();
    assert_eq!(dyns.len(), 2, "{all:?}");
    assert_eq!(
        (dyns[0].target.as_str(), dyns[0].caller.as_deref(), dyns[0].line),
        ("this.m[k]", Some("run"), 3)
    );
    assert_eq!(
        (dyns[1].target.as_str(), dyns[1].caller.as_deref(), dyns[1].line),
        ("h[name]", Some("__module__"), 7)
    );
}

#[test]
fn works_in_a_vue_script() {
    let src = "<script setup lang=\"ts\">\nfunction go(actions: any, name: string) {\n  actions[name]();\n  const m = import(`./m/${name}.js`);\n}\n</script>\n<template><div /></template>\n";
    let all = analyze("/p/C.vue", src);
    let targets: Vec<_> = all
        .iter()
        .filter(|c| c.category == "dynamic_call")
        .map(|c| c.target.as_str())
        .collect();
    assert_eq!(targets, vec!["actions[name]", "import(`./m/${name}.js`)"], "{all:?}");
}
