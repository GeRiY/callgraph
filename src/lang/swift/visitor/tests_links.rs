//! A kapcsolatok tesztjei: metódus-hivatkozás, fogadó-típus feloldás, closure-bekötés, protokoll ↔ implementáció,
//! `.environment(…)` befecskendezés és navigációs célpontok — a teljes frontenden és a hívó-indexen át.

use crate::core::SourceFile;
use crate::index::CallerIndex;
use crate::lang::Language;
use crate::lang::swift::SwiftFrontend;
use crate::model::{Call, FileGraph};

fn graph_of(path: &str, text: &str) -> FileGraph {
    SwiftFrontend::with_defaults()
        .analyze(&SourceFile::new(path, text.as_bytes().to_vec(), None))
        .unwrap()
        .graph
}

fn call<'g>(g: &'g FileGraph, target: &str) -> &'g Call {
    g.calls
        .iter()
        .find(|c| c.target == target)
        .unwrap_or_else(|| panic!("no call {target}: {:?}", g.calls))
}

const SCREEN: &str = r#"import SwiftUI

struct ChatScreen: View {
    @State private var vm = ChatViewModel()
    var body: some View {
        Composer(onSend: vm.send)
            .task { await vm.appear() }
    }
}

@Observable final class ChatViewModel {
    func send(_ text: String) { print(text) }
    func appear() async {}
}
"#;

#[test]
fn method_reference_is_a_ref_edge_and_the_receiver_type_comes_from_the_declaration() {
    let g = graph_of("/p/ChatScreen.swift", SCREEN);
    let reference = call(&g, "vm.send");
    assert_eq!(reference.resolved.as_deref(), Some("ChatViewModel::send"));
    assert_eq!(reference.handler, Some(true), "{reference:?}");
    assert_eq!(
        (reference.caller.as_deref(), reference.container.as_deref()),
        (Some("body"), Some("ChatScreen"))
    );
    assert_eq!(reference.on.as_deref(), Some("vm"));
    // a valódi hívás ugyanígy feloldódik, de nem hivatkozás
    let invoked = call(&g, "vm.appear");
    assert_eq!(invoked.resolved.as_deref(), Some("ChatViewModel::appear"));
    assert_eq!(invoked.handler, None);
    assert_eq!(invoked.on.as_deref(), Some("vm"));
}

#[test]
fn receiver_types_from_let_bindable_params_and_locals() {
    let g = graph_of(
        "/p/A.swift",
        r#"import SwiftUI
struct A: View {
    let vm: ChatViewModel
    var body: some View { Composer(onSend: vm.send) }
}
struct B: View {
    @Bindable var vm: ChatViewModel
    var body: some View { Button("x", action: vm.send) }
}
final class C {
    let other: Other
    init(vm: ChatViewModel, other: Other) { self.other = other; vm.appear() }
    func go() {
        let local = Store()
        local.load()
        other.run()
        self.other.stop()
        helper.unknown()
    }
}
"#,
    );
    let sends: Vec<&Call> = g.calls.iter().filter(|c| c.target == "vm.send").collect();
    assert_eq!(sends.len(), 2);
    assert!(sends.iter().all(|c| c.handler == Some(true)));
    assert!(
        sends
            .iter()
            .all(|c| c.resolved.as_deref() == Some("ChatViewModel::send"))
    );
    assert_eq!(
        call(&g, "vm.appear").resolved.as_deref(),
        Some("ChatViewModel::appear")
    );
    assert_eq!(
        call(&g, "local.load").resolved.as_deref(),
        Some("Store::load")
    );
    assert_eq!(
        call(&g, "other.run").resolved.as_deref(),
        Some("Other::run")
    );
    assert_eq!(
        call(&g, "self.other.stop").resolved.as_deref(),
        Some("Other::stop")
    );
    // ismeretlen fogadó: feloldatlan marad
    assert_eq!(call(&g, "helper.unknown").resolved, None);
}

#[test]
fn property_reads_and_plain_values_are_not_references() {
    let g = graph_of(
        "/p/A.swift",
        r#"import SwiftUI
struct A: View {
    @State private var vm = ChatViewModel()
    var body: some View { Text(vm.title) }
}
"#,
    );
    assert!(g.calls.iter().all(|c| c.handler.is_none()), "{:?}", g.calls);
    assert!(g.calls.iter().all(|c| c.target != "vm.title"));
}

const SOCKETS: &str = r#"protocol RealtimeSocket {
    var onFrame: ((Frame) -> Void)? { get set }
    func send(_ data: Data)
}

final class LiveSocket: RealtimeSocket {
    var onFrame: ((Frame) -> Void)?
    func send(_ data: Data) {}
    func pump(_ frame: Frame) {
        onFrame?(frame)
    }
}
"#;

const CONNECTION: &str = r#"final class ChatConnection {
    let socket: RealtimeSocket
    init(socket: RealtimeSocket) { self.socket = socket }
    func start() {
        socket.onFrame = { frame in
            self.handle(frame)
        }
    }
    func handle(_ frame: Frame) {}
    func ping() { socket.send(Data()) }
}
"#;

#[test]
fn closure_assignment_is_a_wire_edge_and_the_stored_closure_call_resolves() {
    let sockets = graph_of("/p/Sockets.swift", SOCKETS);
    let connection = graph_of("/p/Connection.swift", CONNECTION);
    let wire = call(&connection, "socket.onFrame");
    assert_eq!(wire.kind, "wire");
    assert_eq!(wire.resolved.as_deref(), Some("RealtimeSocket::onFrame"));
    assert_eq!(
        (wire.caller.as_deref(), wire.line, wire.end_line),
        (Some("start"), 5, Some(7))
    );
    // a closure hívása a bekötő metódushoz tartozik, mint eddig
    let handled = call(&connection, "self.handle");
    assert_eq!(handled.caller.as_deref(), Some("start"));
    assert_eq!(handled.resolved.as_deref(), Some("ChatConnection::handle"));
    // a tárolt closure hívása: a típus tulajdonsága
    let invoked = call(&sockets, "onFrame");
    assert_eq!(invoked.resolved.as_deref(), Some("LiveSocket::onFrame"));
    assert_eq!(invoked.caller.as_deref(), Some("pump"));
}

#[test]
fn the_invoking_method_is_a_caller_of_the_closures_callees() {
    let sockets = graph_of("/p/Sockets.swift", SOCKETS);
    let connection = graph_of("/p/Connection.swift", CONNECTION);
    let index = CallerIndex::build([
        ("/p/Sockets.swift", &sockets),
        ("/p/Connection.swift", &connection),
    ]);
    let rows = index.callers("ChatConnection::handle", 2).unwrap();
    let first: Vec<(&str, usize)> = rows.iter().map(|r| (r.caller.as_str(), r.level)).collect();
    assert_eq!(first, vec![("start", 1), ("pump", 2)], "{rows:?}");
    assert_eq!(rows[0].via, None);
    // a „via” a callback-tulajdonság teljes neve (melyik típusé a bekötött closure)
    assert_eq!(rows[1].via.as_deref(), Some("RealtimeSocket::onFrame"));
    // a bekötés maga a tulajdonság hívója
    let wired = index.callers("RealtimeSocket::onFrame", 1).unwrap();
    assert_eq!(wired.len(), 1);
    assert_eq!(wired[0].caller, "start");
}

#[test]
fn protocol_and_implementation_callers_see_each_other_with_a_via_mark() {
    let sockets = graph_of("/p/Sockets.swift", SOCKETS);
    let connection = graph_of("/p/Connection.swift", CONNECTION);
    let direct = graph_of(
        "/p/Direct.swift",
        r#"final class Direct {
    let live: LiveSocket
    init(live: LiveSocket) { self.live = live }
    func run() { live.send(Data()) }
}
"#,
    );
    let index = CallerIndex::build([
        ("/p/Sockets.swift", &sockets),
        ("/p/Connection.swift", &connection),
        ("/p/Direct.swift", &direct),
    ]);
    let via_of = |pattern: &str| {
        let mut found: Vec<(String, Option<String>)> = index
            .callers(pattern, 1)
            .unwrap()
            .into_iter()
            .map(|r| (r.caller, r.via))
            .collect();
        found.sort();
        found
    };
    assert_eq!(
        via_of("LiveSocket::send"),
        vec![
            ("ping".to_owned(), Some("RealtimeSocket".to_owned())),
            ("run".to_owned(), None),
        ]
    );
    assert_eq!(
        via_of("RealtimeSocket::send"),
        vec![
            ("ping".to_owned(), None),
            ("run".to_owned(), Some("LiveSocket".to_owned())),
        ]
    );
}

#[test]
fn conformance_in_an_extension_merges_into_the_type() {
    let sockets = graph_of(
        "/p/Sockets.swift",
        r#"protocol RealtimeSocket { func send(_ data: Data) }
final class LiveSocket {}
extension LiveSocket: RealtimeSocket {
    func send(_ data: Data) {}
}
"#,
    );
    let direct = graph_of(
        "/p/Direct.swift",
        r#"final class Direct {
    let live: LiveSocket
    func run() { live.send(Data()) }
}
"#,
    );
    let index = CallerIndex::build([("/p/Sockets.swift", &sockets), ("/p/Direct.swift", &direct)]);
    let rows = index.callers("RealtimeSocket::send", 1).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].via.as_deref(), Some("LiveSocket"));
}

#[test]
fn environment_injection_names_the_injected_type() {
    let g = graph_of(
        "/p/Root.swift",
        r#"import SwiftUI
struct Root: View {
    @State private var store = Store()
    var body: some View {
        ContentView()
            .environment(store)
            .environment(\.locale, locale)
            .environmentObject(Session())
    }
}
struct ContentView: View {
    @Environment(Store.self) private var store
    var body: some View { Text("x").onAppear { store.load() } }
}
"#,
    );
    let mut injected: Vec<(&str, &str)> = g
        .calls
        .iter()
        .filter(|c| c.kind == "inject")
        .map(|c| (c.target.as_str(), c.caller.as_deref().unwrap_or("")))
        .collect();
    injected.sort_unstable();
    assert_eq!(
        injected,
        vec![
            (".environment(Store)", "body"),
            (".environmentObject(Session)", "body")
        ],
        "{:?}",
        g.calls
    );
    // a fogadó oldal: `@Environment(Store.self)` adja a property típusát
    assert_eq!(
        call(&g, "store.load").resolved.as_deref(),
        Some("Store::load")
    );
}

#[test]
fn views_built_in_navigation_closures_are_navigation_edges() {
    let g = graph_of(
        "/p/Host.swift",
        r#"import SwiftUI
struct Host: View {
    var body: some View {
        NavigationStack {
            List { Row() }
                .navigationDestination(for: Item.self) { item in
                    DetailView(item: item)
                }
                .sheet(isPresented: $show) { SettingsView() }
        }
    }
}
"#,
    );
    assert_eq!(call(&g, "DetailView").kind, "nav");
    assert_eq!(call(&g, "SettingsView").kind, "nav");
    assert_eq!(call(&g, "Row").kind, "new");
    assert_eq!(call(&g, "DetailView").category, "component_use");
}
