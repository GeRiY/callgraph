//! A Swift-visitor tesztjei a teljes frontenden át.

use crate::core::SourceFile;
use crate::lang::Language;
use crate::lang::swift::SwiftFrontend;
use crate::model::FileGraph;

fn graph(text: &str) -> FileGraph {
    SwiftFrontend::with_defaults()
        .analyze(&SourceFile::new(
            "/p/Sample.swift",
            text.as_bytes().to_vec(),
            None,
        ))
        .unwrap()
        .graph
}

fn call<'g>(g: &'g FileGraph, target: &str) -> &'g crate::model::Call {
    g.calls
        .iter()
        .find(|c| c.target == target)
        .unwrap_or_else(|| panic!("no call {target}: {:?}", g.calls))
}

const VIEW: &str = r#"import SwiftUI

/// Main screen.
struct ContentView: View {
    @State private var name: String = ""
    @StateObject var model = Model()
    static let title = "Hi"

    var body: some View {
        VStack {
            Text(name)
                .padding()
            Row(item: name)
        }
        .onAppear { reload() }
    }

    private func reload() {
        // TODO: debounce
        model.load()
        print("reloading \(name)")
    }
}
"#;

#[test]
fn view_struct_is_component_with_wrappers() {
    let g = graph(VIEW);
    assert_eq!(g.framework.as_deref(), Some("swiftui"));
    assert_eq!(g.imports.get("SwiftUI"), Some("SwiftUI"));
    let c = &g.containers[0];
    assert_eq!(
        (c.kind.as_str(), c.name.as_str()),
        ("component", "ContentView")
    );
    assert_eq!(c.implements.as_deref(), Some(&["View".to_string()][..]));
    assert_eq!(c.doc.as_deref(), Some("Main screen."));
    let name = &g.properties[0];
    assert_eq!(name.type_hint.as_deref(), Some("@State String"));
    assert_eq!(name.visibility, "private");
    let model = &g.properties[1];
    assert_eq!(model.type_hint.as_deref(), Some("@StateObject Model"));
    let title = &g.constants[0];
    assert_eq!(
        (title.name.as_str(), title.value.as_deref()),
        ("title", Some("\"Hi\""))
    );
}

#[test]
fn body_is_a_computed_method_with_swiftui_calls() {
    let g = graph(VIEW);
    let body = g.methods.iter().find(|m| m.name == "body").unwrap();
    assert_eq!(body.kind.as_deref(), Some("computed"));
    assert_eq!(body.return_type.as_deref(), Some("some View"));
    assert_eq!((body.line, body.end_line), (9, Some(16)));
    assert_eq!(call(&g, "VStack").category, "swiftui_view");
    assert_eq!(call(&g, "Text").category, "swiftui_view");
    assert_eq!(call(&g, ".padding").category, "swiftui_modifier");
    assert_eq!(call(&g, ".onAppear").category, "swiftui_modifier");
    assert_eq!(call(&g, "Row").category, "component_use");
    assert_eq!(call(&g, "reload").category, "internal_helper");
}

#[test]
fn method_calls_strings_and_comments() {
    let g = graph(VIEW);
    let load = call(&g, "model.load");
    assert_eq!(
        (
            load.category.as_str(),
            load.on.as_deref(),
            load.caller.as_deref()
        ),
        ("external_call", Some("model"), Some("reload"))
    );
    assert_eq!(call(&g, "print").category, "native_swift");
    assert!(g.strings.iter().any(|s| s.value == "reloading \\(…)"));
    let todo = &g.comments[0];
    assert_eq!(
        (todo.tag.as_deref(), todo.caller.as_deref()),
        (Some("TODO"), Some("reload"))
    );
}

#[test]
fn types_extensions_protocols_and_enums() {
    let g = graph(
        r#"protocol Store { func fetch() throws -> [Int] }
enum Kind: Int, CaseIterable { case a = 1, b
  var label: String { "x" } }
struct Kind2 {}
extension Kind2: Store { func fetch() throws -> [Int] { [] } }
extension Kind2 { func other() {} }
final class Base: NSObject, Store {
    init(x: Int = 1) {}
    func fetch() -> [Int] { [] }
}
actor Worker {}
func helper() {}
"#,
    );
    let kinds: Vec<(&str, &str)> = g
        .containers
        .iter()
        .map(|c| (c.kind.as_str(), c.name.as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("module", "Sample.swift"),
            ("protocol", "Store"),
            ("enum", "Kind"),
            ("struct", "Kind2"),
            ("extension", "Kind2@L5"),
            ("extension", "Kind2@L6"),
            ("class", "Base"),
            ("actor", "Worker"),
        ]
    );
    let base = g.containers.iter().find(|c| c.name == "Base").unwrap();
    assert_eq!(base.extends.as_deref(), Some(&["NSObject".to_string()][..]));
    assert_eq!(base.is_final, Some(true));
    let fetch = g
        .methods
        .iter()
        .find(|m| m.container.as_deref() == Some("Store"))
        .unwrap();
    assert!(fetch.is_abstract);
    assert_eq!(fetch.return_type.as_deref(), Some("[Int]"));
    let init = g.methods.iter().find(|m| m.name == "init").unwrap();
    assert_eq!(init.params, vec!["x: Int = 1".to_string()]);
    let cases: Vec<_> = g
        .constants
        .iter()
        .filter(|c| c.kind == "enum_case")
        .map(|c| (c.name.as_str(), c.value.as_deref()))
        .collect();
    assert_eq!(cases, vec![("a", Some("1")), ("b", None)]);
    let helper = g.methods.iter().find(|m| m.name == "helper").unwrap();
    assert_eq!(
        (helper.container.as_deref(), helper.kind.as_deref()),
        (Some("Sample.swift"), Some("function"))
    );
}

#[test]
fn control_flow_if_switch_guard() {
    let g = graph(
        r#"func f(x: Int) {
    guard x > 0 else { return }
    if x == 1 {
        a()
    } else if x == 2 {
        b()
    } else {
        c()
    }
    switch x {
    case 1: a()
    default: b()
    }
}
"#,
    );
    let kinds: Vec<&str> = g.control_flow.iter().map(|c| c.kind.as_str()).collect();
    assert_eq!(kinds, vec!["guard", "if", "switch"]);
    let labels: Vec<&str> = g.control_flow[1]
        .branches
        .iter()
        .map(|b| b.label.as_str())
        .collect();
    assert_eq!(labels, vec!["if (x == 1)", "elseif (x == 2)", "else"]);
    assert_eq!(g.control_flow[2].branches[1].label, "default");
}
