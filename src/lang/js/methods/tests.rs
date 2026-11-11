use std::path::Path;

use oxc_allocator::Allocator;

use super::MethodsPass;
use crate::lang::js::analysis::{JsAnalysis, JsPass, PassContext};
use crate::lang::js::lines::LineIndex;
use crate::lang::js::parse::{parse_program, source_type_for};

fn methods(path: &str, src: &str, setup: bool) -> Vec<(String, String, u32, u32)> {
    let alloc = Allocator::default();
    let p = Path::new(path);
    let parsed = parse_program(&alloc, src, source_type_for(p, None));
    let lines = LineIndex::new(src);
    let ctx = PassContext {
        path: p,
        source: src,
        lines: &lines,
        setup,
    };
    let mut out = JsAnalysis::new(path);
    MethodsPass.run(&ctx, &parsed.program, &mut out);
    out.builder
        .build()
        .methods
        .into_iter()
        .map(|m| {
            (
                m.name,
                m.kind.unwrap_or_default(),
                m.line,
                m.end_line.unwrap_or(0),
            )
        })
        .collect()
}

fn names(v: &[(String, String, u32, u32)]) -> Vec<&str> {
    v.iter().map(|m| m.0.as_str()).collect()
}

#[test]
fn top_level_functions_wrappers_and_nested() {
    let src = "export function a() {\n  function b() {}\n  const c = () => 1\n}\nconst d = computed(() => 1)\nonMounted(() => {})\n";
    let m = methods("/p/x.js", src, false);
    assert_eq!(
        names(&m),
        ["a", "__module__", "a.b", "a.c", "d", "onMounted@L6"]
    );
    assert_eq!(m[4].1, "computed");
    assert_eq!(m[5].1, "hook");
}

#[test]
fn name_collision_gets_line_suffix() {
    let m = methods("/p/x.js", "function a() {}\nfunction a() {}\n", false);
    assert_eq!(names(&m), ["a", "a@L2"]);
}

#[test]
fn options_api_members() {
    let src = "export default {\n  setup() { function h() {} },\n  methods: { go() {} },\n  computed: { total: { get() { return 1 } } },\n  props: { a: String },\n}\n";
    let m = methods("/p/O.vue", src, false);
    assert_eq!(names(&m), ["setup", "setup.h", "go", "computed.total"]);
}

#[test]
fn class_methods_and_container() {
    let m = methods(
        "/p/c.ts",
        "export class A extends B {\n  static s() {}\n  p = () => 1\n}\n",
        false,
    );
    assert_eq!(names(&m), ["s", "p"]);
}

#[test]
fn pseudo_method_only_with_top_level_call() {
    let with = methods("/p/S.vue", "const a = ref(0)\n", true);
    assert_eq!(names(&with), ["setup"]);
    let without = methods("/p/S.vue", "const a = 1\nfunction f() { g() }\n", true);
    assert_eq!(names(&without), ["f"]);
}

#[test]
fn object_literal_methods() {
    let src = "export const api = {\n  fetch() { return 1 },\n  save: () => 2,\n  load: function () {},\n  nested: { deep() {} },\n  value: 3,\n}\nconst other = { run() {} }\n";
    let m = methods("/p/api.js", src, false);
    assert_eq!(
        names(&m),
        [
            "api.fetch",
            "api.save",
            "api.load",
            "api.nested.deep",
            "other.run"
        ]
    );
    assert_eq!(m[0].1, "method");
    assert_eq!((m[0].2, m[0].3), (2, 2));
}

#[test]
fn plain_export_default_object_is_not_options_api() {
    let src = "export default {\n  fetch() {},\n  helpers: { fmt() {} },\n}\n";
    let m = methods("/p/svc.ts", src, false);
    assert_eq!(names(&m), ["fetch", "helpers.fmt"]);
    assert_eq!(m[0].1, "method");
}

#[test]
fn ts_return_type_is_captured() {
    let src = "export function f(a: number): Promise<string[]> { return g(); }\nexport const h = (x: string): boolean => true;\n";
    let alloc = Allocator::default();
    let p = Path::new("/p/r.ts");
    let parsed = parse_program(&alloc, src, source_type_for(p, None));
    let lines = LineIndex::new(src);
    let ctx = PassContext {
        path: p,
        source: src,
        lines: &lines,
        setup: false,
    };
    let mut out = JsAnalysis::new("/p/r.ts");
    MethodsPass.run(&ctx, &parsed.program, &mut out);
    let got: Vec<(String, Option<String>)> = out
        .builder
        .build()
        .methods
        .into_iter()
        .map(|m| (m.name, m.return_type))
        .collect();
    assert!(
        got.contains(&("f".into(), Some("Promise<string[]>".into()))),
        "{got:?}"
    );
    assert!(
        got.contains(&("h".into(), Some("boolean".into()))),
        "{got:?}"
    );
}
