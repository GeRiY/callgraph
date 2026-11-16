//! Valósághű SwiftUI / SwiftData / async minták: metódus-hivatkozások a hívás-szövegkörnyezetek szerint,
//! fájlokon átnyúló fogadó-láncok, property wrapperek, navigációs célpontok, generikus nézetek, actor.

use crate::core::SourceFile;
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

fn refs(g: &FileGraph) -> Vec<&str> {
    let mut found: Vec<&str> = g
        .calls
        .iter()
        .filter(|c| c.handler == Some(true))
        .map(|c| c.target.as_str())
        .collect();
    found.sort_unstable();
    found
}

const REFERENCES: &str = r#"import SwiftUI

struct Item { let id: Int; let fullName: String }

struct Screen: View {
    @StateObject private var vm = ScreenModel()
    @State private var query = ""
    var body: some View {
        VStack {
            Button("Save", action: vm.save)
            Text(vm.title)
        }
        .onAppear(perform: vm.appear)
        .onChange(of: query, vm.changed)
        .refreshable(action: vm.refresh)
        .onReceive(vm.ticker) { _ in }
        .task { await vm.load() }
    }

    func start() {
        Task(operation: vm.run)
        vm.publisher.sink(receiveValue: vm.apply)
        let rows = vm.items.map(Item.init)
        let names = vm.items.map(\.fullName)
        let sorted = vm.items.sorted(by: \.fullName)
    }
}

final class ScreenModel: ObservableObject {
    @Published var title = ""
    var ticker = Ticker()
    var publisher = Publisher()
    var items: [Item] = []
    func save() {}
    func appear() {}
    func changed(_ value: String) {}
    func refresh() async {}
    func load() async {}
    func run() async {}
    func apply(_ value: Int) {}
}
"#;

#[test]
fn method_references_by_call_context() {
    let g = graph_of("/p/Screen.swift", REFERENCES);
    assert_eq!(
        refs(&g),
        vec![
            "Item.init",
            "vm.appear",
            "vm.apply",
            "vm.changed",
            "vm.refresh",
            "vm.run",
            "vm.save"
        ],
        "{:?}",
        g.calls
    );
    assert_eq!(
        call(&g, "vm.changed").resolved.as_deref(),
        Some("ScreenModel::changed")
    );
    assert_eq!(
        call(&g, "Item.init").resolved.as_deref(),
        Some("Item::init")
    );
    // a tulajdonság-olvasás és az `onReceive` publisher-argumentuma érték, nem hivatkozás
    assert!(g.calls.iter().all(|c| c.target != "vm.title"));
    assert!(g.calls.iter().all(|c| c.target != "vm.ticker"));
    // a closure-ös `.task { await vm.load() }` valódi hívás
    let load = call(&g, "vm.load");
    assert_eq!(load.handler, None);
    assert_eq!(load.resolved.as_deref(), Some("ScreenModel::load"));
}

#[test]
fn key_paths_are_not_calls_or_references() {
    let g = graph_of("/p/Screen.swift", REFERENCES);
    assert!(
        g.calls.iter().all(|c| !c.target.contains("fullName")),
        "{:?}",
        g.calls
    );
}

#[test]
fn selector_is_a_reference_to_the_named_method() {
    let g = graph_of(
        "/p/Watcher.swift",
        r#"import UIKit

final class Watcher {
    let model: WatchModel
    init(model: WatchModel) {
        self.model = model
        NotificationCenter.default.addObserver(self, selector: #selector(handle), name: .changed, object: nil)
        NotificationCenter.default.addObserver(self, selector: #selector(model.tick), name: .tick, object: nil)
    }
    @objc func handle() {}
}
"#,
    );
    let handle: Vec<&Call> = g.calls.iter().filter(|c| c.target == "handle").collect();
    assert_eq!(handle.len(), 1, "{:?}", g.calls);
    assert_eq!(handle[0].handler, Some(true));
    assert_eq!(handle[0].caller.as_deref(), Some("init"));
    let tick = call(&g, "model.tick");
    assert_eq!(tick.handler, Some(true));
    assert_eq!(tick.resolved.as_deref(), Some("WatchModel::tick"));
}

#[test]
fn receiver_chain_stalling_in_another_file_is_recorded_for_the_index() {
    let g = graph_of(
        "/p/Screen.swift",
        r#"final class Screen {
    let store: Store
    init(store: Store) { self.store = store }
    func go() {
        self.store.session.user.load()
        store.session.refresh()
        store.reload()
        let local = Store()
        local.session.close()
        helper.unknown()
    }
}
"#,
    );
    let deep = call(&g, "self.store.session.user.load");
    assert_eq!(deep.resolved, None);
    assert_eq!(deep.on.as_deref(), Some("Store::session.user"));
    let one = call(&g, "store.session.refresh");
    assert_eq!(
        (one.resolved.as_deref(), one.on.as_deref()),
        (None, Some("Store::session"))
    );
    assert_eq!(
        call(&g, "local.session.close").on.as_deref(),
        Some("Store::session")
    );
    // a közvetlen tag a fájl nélkül is feloldódik, az ismeretlen fogadó marad a gyökér-névnél
    let direct = call(&g, "store.reload");
    assert_eq!(direct.resolved.as_deref(), Some("Store::reload"));
    assert_eq!(direct.on.as_deref(), Some("store"));
    let unknown = call(&g, "helper.unknown");
    assert_eq!(
        (unknown.resolved.as_deref(), unknown.on.as_deref()),
        (None, Some("helper"))
    );
}

const SWIFTDATA: &str = r#"import SwiftUI
import SwiftData

@Model final class Item { var title: String = "" }

struct ItemsScreen: View {
    @Environment(\.modelContext) private var context
    @Query(sort: \Item.title) private var items: [Item]
    @AppStorage("sort") private var sort = "title"
    @FocusState private var focused: Bool
    @EnvironmentObject var settings: Settings
    @ObservedObject var vm: ItemsModel

    var body: some View {
        List {
            ForEach(items) { item in Row(item: item) }
            ForEach(vm.items) { item in Row(item: item) }
        }
        .toolbar { Button("Add", action: add) }
    }

    private var header: some View {
        HStack { Text(settings.title) }.padding()
    }

    private func add() {
        context.insert(Item())
        try? context.save()
    }
}
"#;

#[test]
fn query_property_is_a_model_edge_and_the_context_is_swiftdata() {
    let g = graph_of("/p/ItemsScreen.swift", SWIFTDATA);
    let query = call(&g, "@Query(Item)");
    assert_eq!(
        (
            query.kind.as_str(),
            query.category.as_str(),
            query.container.as_deref()
        ),
        ("query", "model_use", Some("ItemsScreen"))
    );
    assert_eq!(call(&g, "context.insert").category, "swiftdata_api");
    assert_eq!(call(&g, "context.save").category, "swiftdata_api");
    assert_eq!(call(&g, "Item").category, "instantiation");
    // a keretrendszer-típus nem projekt-szimbólum: nincs feloldott cél
    assert_eq!(call(&g, "context.insert").resolved, None);
}

#[test]
fn property_wrappers_are_kept_in_the_type_hint() {
    let g = graph_of("/p/ItemsScreen.swift", SWIFTDATA);
    let hint = |name: &str| {
        g.properties
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.type_hint.clone())
            .unwrap_or_default()
    };
    assert!(hint("sort").contains("@AppStorage"), "{}", hint("sort"));
    assert!(hint("focused").contains("@FocusState"));
    assert!(hint("focused").contains("Bool"));
    assert!(hint("settings").contains("@EnvironmentObject"));
    assert!(hint("vm").contains("ItemsModel"));
    assert!(hint("items").contains("@Query"));
}

#[test]
fn foreach_rows_and_view_properties_belong_to_their_members() {
    let g = graph_of("/p/ItemsScreen.swift", SWIFTDATA);
    let rows: Vec<&Call> = g.calls.iter().filter(|c| c.target == "Row").collect();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|c| c.category == "component_use" && c.caller.as_deref() == Some("body"))
    );
    // `var header: some View { … }` nézet-tag: a hívásai hozzá tartoznak
    let header = g.methods.iter().find(|m| m.name == "header").unwrap();
    assert_eq!(header.kind.as_deref(), Some("computed"));
    let text = call(&g, "Text");
    assert_eq!(
        (text.caller.as_deref(), text.container.as_deref()),
        (Some("header"), Some("ItemsScreen"))
    );
    assert_eq!(call(&g, ".padding").category, "swiftui_modifier");
    // a bekötött metódus-hivatkozás
    let add = call(&g, "add");
    assert_eq!(
        (add.handler, add.caller.as_deref()),
        (Some(true), Some("body"))
    );
}

#[test]
fn navigation_destination_switch_yields_a_nav_edge_per_view() {
    let g = graph_of(
        "/p/Root.swift",
        r#"import SwiftUI

enum Route: Hashable { case detail(Int), settings }

@Observable final class Router { var path: [Route] = [] }

struct Root: View {
    @State private var router = Router()
    var body: some View {
        NavigationStack(path: $router.path) {
            Home()
                .navigationDestination(for: Route.self) { route in
                    switch route {
                    case .detail(let id): DetailView(id: id)
                    case .settings: SettingsView()
                    }
                }
        }
    }
}

struct Home: View {
    @Environment(Router.self) private var router
    var body: some View {
        Button("Go") { router.path.append(Route.detail(1)) }
    }
}
"#,
    );
    assert_eq!(call(&g, "DetailView").kind, "nav");
    assert_eq!(call(&g, "SettingsView").kind, "nav");
    assert_eq!(call(&g, "Home").kind, "new");
    // az enum-minta (`case .detail(let id)`) nem hívás
    assert!(
        g.calls.iter().all(|c| c.target != ".detail"),
        "{:?}",
        g.calls
    );
    let append = call(&g, "router.path.append");
    assert_eq!(append.resolved, None);
    assert_eq!(
        call(&g, "Route.detail").resolved.as_deref(),
        Some("Route::detail")
    );
}

#[test]
fn generic_view_instantiation_is_a_component_use() {
    let g = graph_of(
        "/p/Host.swift",
        r#"import SwiftUI

struct ItemList<Element: Identifiable>: View {
    let items: [Element]
    var body: some View { List(items) { Text("x") } }
}

struct Host: View {
    var body: some View { ItemList<Item>(items: stored) }
}
"#,
    );
    let list = call(&g, "ItemList");
    assert_eq!(
        (
            list.kind.as_str(),
            list.category.as_str(),
            list.caller.as_deref()
        ),
        ("new", "component_use", Some("body"))
    );
    assert_eq!(list.container.as_deref(), Some("Host"));
}

#[test]
fn actor_methods_resolve_and_async_let_calls_belong_to_the_caller() {
    let g = graph_of(
        "/p/Loader.swift",
        r#"actor Cache {
    private var store: [String: Data] = [:]
    func get(_ key: String) -> Data? { store[key] }
}

final class Loader {
    let cache: Cache
    let vm: ItemsModel
    init(cache: Cache, vm: ItemsModel) { self.cache = cache; self.vm = vm }
    func run() async {
        let data = await cache.get("a")
        async let first = vm.load()
        async let second = vm.reload()
        _ = await (first, second)
        Task { @MainActor in
            self.vm.finish()
        }
    }
}
"#,
    );
    let cache = g.containers.iter().find(|c| c.name == "Cache").unwrap();
    assert_eq!(cache.kind, "actor");
    let get = call(&g, "cache.get");
    assert_eq!(get.resolved.as_deref(), Some("Cache::get"));
    assert_eq!(get.caller.as_deref(), Some("run"));
    for (target, resolved) in [
        ("vm.load", "ItemsModel::load"),
        ("vm.reload", "ItemsModel::reload"),
        ("self.vm.finish", "ItemsModel::finish"),
    ] {
        let c = call(&g, target);
        assert_eq!(c.resolved.as_deref(), Some(resolved));
        assert_eq!(c.caller.as_deref(), Some("run"));
    }
    assert_eq!(call(&g, "Task").caller.as_deref(), Some("run"));
}
