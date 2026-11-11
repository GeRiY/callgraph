//! A dinamikus (`dynamic_call`) hívás-jelölés tesztjei Python forráson: a pozitív minták, a literál (nem
//! dinamikus) esetek és a közönséges hívások változatlansága.

use crate::core::SourceFile;
use crate::lang::Language;
use crate::lang::python::PythonFrontend;
use crate::model::Call;

fn calls_of(src: &str) -> Vec<Call> {
    PythonFrontend::with_defaults()
        .analyze(&SourceFile::new(
            "/p/sample.py",
            src.as_bytes().to_vec(),
            None,
        ))
        .unwrap()
        .graph
        .calls
}

/// A `dynamic_call` jelölések céljai egy `def f(obj, name, expr):` törzsében.
fn dynamic(body: &str) -> Vec<String> {
    let src = format!("def f(obj, name, expr):\n    {body}\n");
    calls_of(&src)
        .into_iter()
        .filter(|c| c.category == "dynamic_call")
        .map(|c| c.target)
        .collect()
}

fn none(body: &str) {
    assert_eq!(dynamic(body), Vec::<String>::new(), "{body}");
}

#[test]
fn getattr_with_non_literal_name() {
    assert_eq!(dynamic("getattr(obj, name)(1)"), vec!["getattr(obj, name)"]);
    assert_eq!(
        dynamic("getattr(obj, name, None)()"),
        vec!["getattr(obj, name, None)"]
    );
    assert_eq!(
        dynamic("getattr(obj, 'on_' + name)()"),
        vec!["getattr(obj, 'on_' + name)"]
    );
    assert_eq!(
        dynamic("getattr(obj, f'on_{name}')()"),
        vec!["getattr(obj, f'on_{name}')"]
    );
}

#[test]
fn getattr_with_literal_name_is_not_dynamic() {
    none("getattr(obj, 'run')()");
    none("getattr(obj, \"run\")(1)");
    none("getattr(obj, f'run')()");
    none("getattr(obj, 'a' 'b')()");
}

#[test]
fn getattr_alone_is_an_ordinary_call() {
    // a hívás eredménye nincs azonnal hívva: nem dinamikus hívás
    none("m = getattr(obj, name)");
    none("getattr(obj, name)");
}

#[test]
fn getattr_marker_is_one_entry_with_its_inner_call() {
    let all = calls_of("def f(obj, name):\n    getattr(obj, name)(1)\n");
    let dyns: Vec<_> = all
        .iter()
        .filter(|c| c.category == "dynamic_call")
        .collect();
    assert_eq!(dyns.len(), 1, "{all:?}");
    let c = dyns[0];
    assert_eq!((c.kind.as_str(), c.line), ("dynamic", 2));
    assert!(c.resolved.is_none() && c.on.is_none());
    assert_eq!(c.caller.as_deref(), Some("f"));
    // a belső getattr(...) közönséges hívás marad
    assert!(
        all.iter()
            .any(|c| c.target == "getattr" && c.category != "dynamic_call")
    );
}

#[test]
fn globals_and_locals_lookup() {
    assert_eq!(dynamic("globals()[name](1)"), vec!["globals()[name]"]);
    assert_eq!(dynamic("locals()[expr]()"), vec!["locals()[expr]"]);
    assert_eq!(
        dynamic("globals()['handle_' + name]()"),
        vec!["globals()['handle_' + name]"]
    );
}

#[test]
fn globals_lookup_with_literal_key_is_not_dynamic() {
    none("globals()['handler']()");
    none("locals()[\"handler\"]()");
}

#[test]
fn other_subscript_calls_are_not_dynamic() {
    none("handlers[name]()");
    none("obj.table[name](1)");
    none("globals()[name]");
}

#[test]
fn dynamic_import() {
    assert_eq!(
        dynamic("importlib.import_module(name)"),
        vec!["importlib.import_module(name)"]
    );
    assert_eq!(
        dynamic("importlib.import_module('pkg.' + name)"),
        vec!["importlib.import_module('pkg.' + name)"]
    );
    assert_eq!(dynamic("__import__(name)"), vec!["__import__(name)"]);
    assert_eq!(
        dynamic("importlib.import_module(name=expr)"),
        vec!["importlib.import_module(name=expr)"]
    );
}

#[test]
fn dynamic_import_with_literal_argument_is_not_dynamic() {
    none("importlib.import_module('json')");
    none("importlib.import_module(\"pkg.mod\")");
    none("__import__('os')");
    none("importlib.import_module()");
    none("other.import_module(name)");
    none("importlib.reload(name)");
}

#[test]
fn import_calls_keep_their_ordinary_entry() {
    let all = calls_of("def f(name):\n    importlib.import_module(name)\n");
    assert!(
        all.iter()
            .any(|c| c.target == "importlib.import_module" && c.category != "dynamic_call")
    );
    assert!(all.iter().any(|c| c.category == "dynamic_call"));
}

#[test]
fn eval_and_exec() {
    assert_eq!(dynamic("eval(expr)"), vec!["eval(expr)"]);
    assert_eq!(dynamic("exec(expr)"), vec!["exec(expr)"]);
    assert_eq!(dynamic("eval('1 + 1')"), vec!["eval('1 + 1')"]);
    assert_eq!(dynamic("exec(code, {})"), vec!["exec(code, {})"]);
}

#[test]
fn eval_like_names_are_not_dynamic() {
    none("obj.eval(expr)");
    none("evaluate(expr)");
    none("literal_eval(expr)");
    none("ast.literal_eval(expr)");
}

#[test]
fn ordinary_calls_are_unchanged() {
    none("obj.run(1); helper(2); Foo(3); len(expr); print(name); super().run()");
    none("getattr(obj, 'x'); setattr(obj, name, 1); hasattr(obj, name)");
    none("fn = lambda: 1; fn()");
    let all = calls_of("def f(obj):\n    obj.run()\n    helper()\n    Foo()\n");
    let kinds: Vec<_> = all.iter().map(|c| c.kind.as_str()).collect();
    assert_eq!(kinds, vec!["method_call", "func_call", "new"]);
}

#[test]
fn long_expression_is_shortened() {
    let long = format!("getattr(obj, {})", "name + ".repeat(20) + "name");
    let found = dynamic(&format!("{long}()"));
    assert_eq!(found.len(), 1);
    assert!(found[0].chars().count() <= 60, "{}", found[0]);
    assert!(found[0].ends_with('…'));
}

#[test]
fn works_in_methods_and_at_module_level() {
    let all = calls_of(
        "class A:\n    def run(self, name):\n        getattr(self, name)()\n\nglobals()[x]()\n",
    );
    let dyns: Vec<_> = all
        .iter()
        .filter(|c| c.category == "dynamic_call")
        .collect();
    assert_eq!(dyns.len(), 2, "{all:?}");
    assert_eq!(
        (
            dyns[0].target.as_str(),
            dyns[0].container.as_deref(),
            dyns[0].caller.as_deref()
        ),
        ("getattr(self, name)", Some("A"), Some("run"))
    );
    assert_eq!(dyns[1].target, "globals()[x]");
    assert_eq!(dyns[1].line, 5);
}
