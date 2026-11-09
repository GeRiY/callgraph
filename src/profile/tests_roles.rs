//! A hívásirány-lánc (csak megfigyelt párok, mellékágak), az enum-fájlok és a `mail` / `notification` szerepek
//! egységtesztjei — fájlrendszer nélkül (a segédek a `tests_layers.rs`-ből).

use serde_json::json;

use super::build::settle;
use super::classify::Classifier;
use super::flow::{FlowFile, compute};
use super::format::{Tip, render};
use super::project::ProjectProfile;
use super::role::Role;
use super::tests_layers::{call, class, entries, graph};
use crate::i18n::Lang;
use crate::index::TypeHierarchy;
use crate::model::FileGraph;

#[test]
fn flow_chain_shows_only_observed_pairs_and_side_branches() {
    let controller = graph(json!({
        "namespace": "App\\Http\\Controllers",
        "containers": [class("OrderController")],
        "calls": [call("OrderController", "App\\Services\\OrderService::find"),
                  call("OrderController", "App\\Services\\OrderService::all")]
    }));
    let service = graph(json!({
        "namespace": "App\\Services",
        "containers": [class("OrderService")],
        "calls": [call("OrderService", "App\\Models\\Order::find"),
                  call("OrderService", "App\\Models\\Order::all")]
    }));
    let model = graph(json!({
        "namespace": "App\\Models",
        "containers": [class("Order")]
    }));
    let command = graph(json!({
        "namespace": "App\\Console\\Commands",
        "containers": [class("SyncOrders")],
        "calls": [call("SyncOrders", "App\\Services\\OrderService::all")]
    }));
    let c = Classifier::default();
    let files = [
        ("app/Http/Controllers/OrderController.php", &controller),
        ("app/Services/OrderService.php", &service),
        ("app/Models/Order.php", &model),
        ("app/Console/Commands/SyncOrders.php", &command),
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
    let chain: Vec<Vec<&str>> = flow
        .chain
        .iter()
        .map(|group| group.iter().map(|l| l.role.name()).collect())
        .collect();
    // nincs controller → command hívás: a command mellékág, nem a fő lánc tagja
    assert_eq!(
        chain,
        [
            vec!["controller", "service", "model"],
            vec!["command", "service"]
        ]
    );
    let paths: Vec<&str> = files.iter().map(|(rel, _)| *rel).collect();
    let profile = ProjectProfile::aggregate(&entries(&paths), Vec::new(), Some(flow), true);
    let out = render(&profile, Lang::En, false, Tip::Hide);
    assert!(
        out.contains("flow: controller → service → model; command → service   ("),
        "{out}"
    );
}

#[test]
fn enum_files_get_no_role_from_their_name() {
    let c = Classifier::default();
    let hierarchy = TypeHierarchy::from_graphs([]);
    let enum_graph = graph(json!({
        "namespace": "App\\Support",
        "containers": [{"type": "enum", "name": "PaymentProvider", "line": 1, "endLine": 9, "doc": null}]
    }));
    let class_graph = graph(json!({
        "namespace": "App\\Support",
        "containers": [class("PaymentProvider")]
    }));
    let settled = |rel: &str, file_graph: &FileGraph| {
        let mut entry = entries(&[rel]).remove(0);
        settle(&mut entry, Some(file_graph), &hierarchy, &c);
        entry.class.role.map(Role::name)
    };
    // csak az útból: az `Enums` mappa dönt (a név nem); más mappában a gráf mondja meg, hogy enum
    assert_eq!(
        c.classify("app/Enums/PaymentProvider.php")
            .unwrap()
            .role
            .map(Role::name),
        Some("enum")
    );
    assert_eq!(
        settled("app/Support/PaymentProvider.php", &enum_graph),
        None
    );
    assert_eq!(
        settled("app/Support/PaymentProvider.php", &class_graph),
        Some("provider")
    );
    // a mappa-szerep enumnál is marad
    assert_eq!(
        settled("app/Models/PaymentProvider.php", &enum_graph),
        Some("model")
    );
}

#[test]
fn mail_and_notification_need_a_code_signal() {
    let c = Classifier::default();
    let hierarchy = TypeHierarchy::from_graphs([]);
    let mailable = graph(json!({
        "namespace": "App\\Mail",
        "containers": [{"type": "class", "name": "OrderShipped", "line": 1, "endLine": 9,
            "extends": ["Illuminate\\Mail\\Mailable"], "doc": null}]
    }));
    let plain = graph(json!({
        "namespace": "App\\Mail",
        "containers": [class("Inbox")]
    }));
    let notification = graph(json!({
        "namespace": "App\\Notifications",
        "containers": [{"type": "class", "name": "InvoicePaid", "line": 1, "endLine": 9,
            "extends": ["Illuminate\\Notifications\\Notification"], "doc": null}]
    }));
    let settled = |rel: &str, file_graph: &FileGraph| {
        let mut entry = entries(&[rel]).remove(0);
        settle(&mut entry, Some(file_graph), &hierarchy, &c);
        (entry.class.role.map(Role::name), entry.confirmed)
    };
    assert_eq!(
        settled("app/Mail/OrderShipped.php", &mailable),
        (Some("mail"), Some("extends Mailable"))
    );
    assert_eq!(settled("app/Mail/Inbox.php", &plain).0, None);
    assert_eq!(
        settled("app/Notifications/InvoicePaid.php", &notification),
        (Some("notification"), Some("extends Notification"))
    );
}
