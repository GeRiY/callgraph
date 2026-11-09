//! A réteg = szerep + ökoszisztéma (minősítő, szűrő, hívásirány), a fájlszám szerinti rendezés, a kód-jelet kívánó
//! szerepek (`event`, `listener`, `job`) és a `--by-layer` csoportok egységtesztjei — fájlrendszer nélkül.

use std::path::Path;

use serde_json::json;

use super::build::settle;
use super::classify::Classifier;
use super::flow::{FlowFile, compute};
use super::format::{Tip, render};
use super::layer::{Layer, LayerFilter, Mixed};
use super::project::{FileEntry, ProjectProfile};
use super::role::Role;
use super::rules::Eco;
use super::scope::LayerScope;
use super::view::{Count, group_header};
use crate::i18n::Lang;
use crate::index::TypeHierarchy;
use crate::model::FileGraph;

fn entries(paths: &[&str]) -> Vec<FileEntry> {
    let c = Classifier::default();
    paths
        .iter()
        .filter_map(|rel| {
            Some(FileEntry {
                rel: (*rel).to_owned(),
                class: c.classify(rel)?,
                confirmed: None,
            })
        })
        .collect()
}

fn graph(value: serde_json::Value) -> FileGraph {
    let mut base = json!({
        "file": "x", "framework": null, "namespace": null, "imports": {}, "containers": [],
        "properties": [], "constants": [], "methods": [], "calls": [], "comments": [],
        "controlFlow": [], "strings": [], "writes": []
    });
    for (key, value) in value.as_object().unwrap() {
        base[key.as_str()] = value.clone();
    }
    serde_json::from_value(base).unwrap()
}

fn call(container: &str, resolved: &str) -> serde_json::Value {
    json!({"container": container, "caller": "run", "line": 3, "type": "method",
        "target": "x", "resolved": resolved, "category": "internal"})
}

fn class(name: &str) -> serde_json::Value {
    json!({"type": "class", "name": name, "line": 1, "endLine": 9, "doc": null})
}

fn named(name: &str) -> Role {
    Role::named(name).unwrap()
}

#[test]
fn a_role_in_two_ecosystems_gets_a_qualifier() {
    let files = entries(&[
        "modules/shop/controllers/CartController.php",
        "modules/shop/controllers/OrderController.php",
        "web/assets/frontend/js/controllers/cart.js",
        "modules/shop/models/Cart.php",
    ]);
    let profile = ProjectProfile::aggregate(&files, Vec::new(), None, false);
    let layers: Vec<(&str, usize)> = profile
        .layers
        .iter()
        .map(|l| (l.label.as_str(), l.files))
        .collect();
    assert_eq!(
        layers,
        [
            ("controller (php)", 2),
            ("controller (js)", 1),
            ("model", 1)
        ]
    );
    let out = render(&profile, Lang::En, false, Tip::Hide);
    assert!(out.contains("\n  controller (php)  "), "{out}");
    assert!(out.contains("\n  controller (js)   "), "{out}");
    // egy ökoszisztémában nincs minősítő
    let single = ProjectProfile::aggregate(
        &entries(&["app/Http/Controllers/A.php", "app/Models/B.php"]),
        Vec::new(),
        None,
        false,
    );
    assert!(single.layers.iter().all(|l| !l.label.contains('(')));
    assert_eq!(single.mixed, Mixed::default());
}

#[test]
fn layer_filter_accepts_one_ecosystem_or_all() {
    let any = LayerFilter::parse("controller").unwrap();
    assert_eq!(any.eco, None);
    assert_eq!(
        LayerFilter::parse("controller:js").unwrap().eco,
        Some(Eco::Js)
    );
    assert_eq!(
        LayerFilter::parse("Controllers (PHP)").unwrap().eco,
        Some(Eco::Php)
    );
    assert_eq!(LayerFilter::parse("service:ts").unwrap().eco, Some(Eco::Js));
    assert!(LayerFilter::parse("controller:cobol").is_none());
    assert!(LayerFilter::parse("nope").is_none());
    let js = Layer::new(named("controller"), Eco::Js);
    assert!(any.matches(js));
    assert!(LayerFilter::parse("controller:js").unwrap().matches(js));
    assert!(!LayerFilter::parse("controller:php").unwrap().matches(js));
    assert!(!LayerFilter::parse("service").unwrap().matches(js));
}

#[test]
fn flow_keeps_backend_and_frontend_services_apart() {
    let controller = graph(json!({
        "namespace": "App\\Http\\Controllers",
        "containers": [class("UserController")],
        "calls": [call("UserController", "App\\Services\\UserService::find")]
    }));
    let service = graph(json!({
        "namespace": "App\\Services",
        "containers": [class("UserService")]
    }));
    let component = graph(json!({
        "module": "src/components/Card.vue",
        "calls": [call("Card", "src/services/api.ts::get")]
    }));
    let api = graph(json!({"module": "src/services/api.ts"}));
    let c = Classifier::default();
    let files = [
        ("app/Http/Controllers/UserController.php", &controller),
        ("app/Services/UserService.php", &service),
        ("src/components/Card.vue", &component),
        ("src/services/api.ts", &api),
    ];
    let flow_files: Vec<FlowFile<'_>> = files
        .iter()
        .map(|(rel, graph)| FlowFile {
            rel,
            layer: c.layer_of(rel),
            graph,
        })
        .collect();
    let flow = compute(&flow_files).unwrap();
    let service_php = Layer::new(named("service"), Eco::Php);
    let service_js = Layer::new(named("service"), Eco::Js);
    assert_eq!(
        flow.chain,
        [
            vec![Layer::new(named("controller"), Eco::Php), service_php],
            vec![Layer::new(named("component"), Eco::Js), service_js]
        ]
    );
    let mixed = Mixed::of(flow.chain.iter().flatten().copied());
    assert_eq!(service_php.label(&mixed), "service (php)");
    assert_eq!(service_js.label(&mixed), "service (js)");
}

#[test]
fn the_main_layer_is_not_hidden_behind_small_ones() {
    let mut paths: Vec<String> = (0..5).map(|i| format!("app/Models/M{i}.php")).collect();
    paths.extend(
        [
            "app/Http/Middleware/A.php",
            "app/Policies/P.php",
            "app/Observers/O.php",
            "app/Handlers/H.php",
            "app/Commands/C.php",
            "app/DTO/D.php",
            "routes/web.php",
            "app/Providers/X.php",
            "tests/T.php",
            "config/app.php",
            "app/Services/S.php",
        ]
        .map(str::to_owned),
    );
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let profile = ProjectProfile::aggregate(&entries(&refs), Vec::new(), None, false);
    assert!(profile.layers.len() > 9);
    assert_eq!(profile.layers[0].label, "model");
    let out = render(&profile, Lang::En, false, Tip::Hide);
    assert!(out.contains("\n  model "), "{out}");
    assert!(out.contains("more layers (profile --full)"), "{out}");
}

#[test]
fn event_listener_and_job_need_a_code_signal() {
    let c = Classifier::default();
    let role = |rel: &str| c.classify(rel).unwrap().role.map(Role::name);
    let fallback = |rel: &str| {
        c.classify(rel)
            .unwrap()
            .fallback
            .map(|f| f.role.map(Role::name))
    };
    // az út egyedül (a `--layer` szűrő) a mappanevet adja; a profil tartaléka a következő jel
    assert_eq!(role("app/Services/Event/Planner.php"), Some("event"));
    assert_eq!(
        fallback("app/Services/Event/Planner.php"),
        Some(Some("service"))
    );
    assert_eq!(
        fallback("web/assets/frontend/js/controllers/event/list.js"),
        Some(Some("controller"))
    );
    assert_eq!(fallback("app/Models/Job.php"), Some(Some("model")));
    assert_eq!(fallback("app/Events/OrderShipped.php"), Some(None));
    assert_eq!(fallback("app/Http/Controllers/UserController.php"), None);

    let shipped = graph(json!({
        "namespace": "App\\Events",
        "containers": [{"type": "class", "name": "OrderShipped", "line": 1, "endLine": 9,
            "traits": ["Illuminate\\Foundation\\Events\\Dispatchable"], "doc": null}]
    }));
    let listener = graph(json!({
        "namespace": "App\\Listeners",
        "containers": [class("SendShipmentMail")],
        "methods": [{"container": "SendShipmentMail", "name": "handle", "line": 3,
            "visibility": "public", "static": false, "abstract": false,
            "params": ["OrderShipped $event"], "returnType": null, "doc": null, "body": null}]
    }));
    let plain = graph(json!({
        "namespace": "App\\Services\\Event",
        "containers": [{"type": "class", "name": "Planner", "line": 1, "endLine": 9, "doc": null}]
    }));
    let hierarchy = TypeHierarchy::from_graphs([&shipped, &listener, &plain]);
    let settled = |rel: &str, file_graph: &FileGraph| {
        let mut entry = entries(&[rel]).remove(0);
        settle(&mut entry, Some(file_graph), &hierarchy);
        (entry.class.role.map(Role::name), entry.confirmed)
    };
    let (event, label) = settled("app/Events/OrderShipped.php", &shipped);
    assert_eq!(event, Some("event"));
    assert!(label.is_some());
    assert_eq!(
        settled("app/Listeners/SendShipmentMail.php", &listener),
        (Some("listener"), Some("handle(…) in Listeners"))
    );
    assert_eq!(
        settled("app/Services/Event/Planner.php", &plain).0,
        Some("service")
    );
    assert_eq!(settled("app/Events/Plain.php", &plain).0, None);
}

#[test]
fn by_layer_runs_and_headers() {
    let root = Path::new("/nonexistent-callgraph-root");
    let scope = LayerScope::new(root, None, true).unwrap();
    let runs = scope.runs([
        "app/Services/A.php",
        "app/Services/B.php",
        "app/Http/Controllers/C.php",
        "web/js/controllers/d.js",
    ]);
    let shown: Vec<(&str, std::ops::Range<usize>)> = runs
        .iter()
        .map(|run| (run.label.as_str(), run.range.clone()))
        .collect();
    assert_eq!(
        shown,
        [
            ("service", 0..2),
            ("controller (php)", 2..3),
            ("controller (js)", 3..4)
        ]
    );
    assert_eq!(
        group_header("service", 25, Count::Calls, Lang::En),
        "[service]  25 calls"
    );
    assert_eq!(
        group_header("service", 1, Count::Callers, Lang::En),
        "[service]  1 caller"
    );
    assert_eq!(
        group_header("service", 3, Count::Calls, Lang::Hu),
        "[service]  3 hívás"
    );
    // a `--by-layer` kulcsa a szerepet is tartalmazza: az azonos rangú rétegek nem keverednek
    let handler = scope.order_key("app/Handlers/H.php");
    let controller = scope.order_key("app/Http/Controllers/C.php");
    assert_ne!(handler, controller);
    let err = LayerScope::new(root, Some("nope"), false).unwrap_err();
    assert!(err.contains("nope") && err.contains("controller:js"), "{err}");
    let only_js = LayerScope::new(root, Some("controller:js"), false).unwrap();
    assert!(only_js.keeps(Path::new("web/js/controllers/d.js")));
    assert!(!only_js.keeps(Path::new("app/Http/Controllers/C.php")));
}
