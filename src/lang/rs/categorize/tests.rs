use super::*;
use crate::lang::rs::module_path::ModulePath;

fn segs(s: &str) -> Vec<String> {
    s.split("::").map(str::to_owned).collect()
}

fn resolver() -> Resolver {
    let mut r = Resolver::new(ModulePath::from_file("src/api/users.rs"));
    r.add_import("Json".into(), segs("axum::Json"));
    r.add_import("Regex".into(), segs("regex::Regex"));
    r.add_import("fs".into(), segs("std::fs"));
    r.add_import("Repo".into(), segs("crate::db::Repo"));
    r.add_import("info".into(), segs("tracing::info"));
    r.add_type("User".into());
    r.add_free_fn("helper", "users");
    r.add_method("save", "User");
    r
}

fn verdict(callee: Callee) -> (&'static str, Option<String>) {
    verdict_bound(callee, false)
}

fn verdict_bound(callee: Callee, bound_locally: bool) -> (&'static str, Option<String>) {
    let r = resolver();
    let v = Categorizer::with_defaults().categorize(&CallInput {
        callee: &callee,
        container: "User",
        inline: &[],
        bound_locally,
        resolver: &r,
    });
    (v.category, v.resolved)
}

fn path(s: &str) -> Callee {
    Callee::Path(segs(s))
}

fn method(name: &str, on_self: bool) -> Callee {
    Callee::Method {
        name: name.into(),
        on_self,
    }
}

#[test]
fn local_items_are_internal_helpers() {
    assert_eq!(
        verdict(path("Self::new")),
        ("internal_helper", Some("User::new".into()))
    );
    assert_eq!(
        verdict(path("helper")),
        ("internal_helper", Some("users::helper".into()))
    );
    assert_eq!(
        verdict(method("save", true)),
        ("internal_helper", Some("User::save".into()))
    );
    assert_eq!(
        verdict(method("save", false)),
        ("internal_helper", Some("User::save".into()))
    );
}

#[test]
fn std_paths_prelude_and_methods_are_native() {
    assert_eq!(
        verdict(path("fs::read")),
        ("native_rust", Some("std::fs::read".into()))
    );
    assert_eq!(verdict(path("String::new")), ("native_rust", None));
    assert_eq!(verdict(path("Some")), ("native_rust", None));
    assert_eq!(verdict(method("unwrap", false)), ("native_rust", None));
}

#[test]
fn framework_crate_and_module_and_package() {
    assert_eq!(
        verdict(path("Json")),
        ("framework_use", Some("axum::Json".into()))
    );
    assert_eq!(verdict(path("tokio::spawn")), ("framework_use", None));
    assert_eq!(verdict(path("Repo::find")), ("module_use", None));
    assert_eq!(
        verdict(path("crate::db::connect")),
        ("module_use", Some("db::connect".into()))
    );
    assert_eq!(
        verdict(path("Regex::new")),
        ("package_use", Some("regex::Regex::new".into()))
    );
    assert_eq!(verdict(path("unknown_fn")), ("module_use", None));
}

#[test]
fn macros_have_their_own_category() {
    let mac = |s: &str| Callee::Macro(segs(s));
    assert_eq!(verdict(mac("println")), ("macro_use", None));
    assert_eq!(
        verdict(mac("info")),
        ("macro_use", Some("tracing::info!".into()))
    );
    assert_eq!(verdict(mac("sqlx::query")), ("macro_use", None));
}

#[test]
fn locals_and_unknown_methods_are_external_calls() {
    assert_eq!(
        verdict_bound(path("callback"), true),
        ("external_call", None)
    );
    assert_eq!(
        verdict(method("frobnicate", false)),
        ("external_call", None)
    );
}

#[test]
fn call_kinds_and_targets() {
    assert_eq!(path("Foo::new").kind(), "static_call");
    assert_eq!(path("std::fs::read").kind(), "func_call");
    assert_eq!(method("x", false).kind(), "method_call");
    assert_eq!(Callee::Macro(segs("vec")).target(), "vec!");
}
