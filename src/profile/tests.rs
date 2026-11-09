//! A szabálytábla, az osztályozó, a `.callgraph-layers`, az összesítő és a formázó egységtesztjei — fájlrendszer
//! nélkül (útlistákon és JSON-ból épített gráfokon).

use super::architecture::ARCHITECTURES;
use super::classify::{Classifier, Reason};
use super::confirm::{CONFIRM_RULES, confirm};
use super::ecosystem::{js_frameworks, rust_frameworks};
use super::flow::{FlowFile, compute};
use super::format::{Tip, render};
use super::layers_file::{Overrides, parse};
use super::project::{FileEntry, ProjectProfile};
use super::role::{Confidence, Role};
use super::rules::{DOTTED_RULES, FOLDER_RULES, SUFFIX_RULES, USE_PREFIX, VUE_FALLBACK};
use crate::i18n::Lang;
use crate::index::TypeHierarchy;
use crate::model::FileGraph;

fn role(rel: &str) -> Option<&'static str> {
    Classifier::default().role_of(rel).map(Role::name)
}

fn confidence(rel: &str) -> Confidence {
    Classifier::default().classify(rel).unwrap().confidence
}

#[test]
fn every_rule_names_a_known_role() {
    let names = FOLDER_RULES
        .iter()
        .map(|r| r.role)
        .chain(SUFFIX_RULES.iter().map(|r| r.role))
        .chain(DOTTED_RULES.iter().map(|r| r.1))
        .chain([VUE_FALLBACK.0, USE_PREFIX.0])
        .chain(CONFIRM_RULES.iter().map(|r| r.role))
        .chain(
            ARCHITECTURES
                .iter()
                .flat_map(|a| a.all.iter().chain(a.any).copied()),
        );
    for name in names {
        assert!(
            Role::named(name).is_some(),
            "unknown role in a rule: {name}"
        );
    }
}

#[test]
fn laravel_folders_and_suffixes() {
    assert_eq!(
        role("app/Http/Controllers/UserController.php"),
        Some("controller")
    );
    assert_eq!(
        confidence("app/Http/Controllers/UserController.php"),
        Confidence::High
    );
    assert_eq!(
        role("app/Http/Controllers/Api/UserController.php"),
        Some("controller")
    );
    assert_eq!(
        role("app/Http/Requests/StoreUserRequest.php"),
        Some("request")
    );
    assert_eq!(
        role("app/Http/Middleware/Authenticate.php"),
        Some("middleware")
    );
    assert_eq!(role("app/Http/Resources/UserResource.php"), Some("dto"));
    assert_eq!(role("app/Console/Commands/SendMails.php"), Some("command"));
    assert_eq!(
        role("app/Providers/AppServiceProvider.php"),
        Some("provider")
    );
    assert_eq!(role("app/Models/User.php"), Some("model"));
    assert_eq!(
        role("app/Repositories/UserRepositoryInterface.php"),
        Some("repository")
    );
    assert_eq!(role("database/factories/UserFactory.php"), Some("factory"));
    assert_eq!(
        role("database/migrations/2024_01_01_create_users.php"),
        Some("migration")
    );
    assert_eq!(role("resources/views/users/index.blade.php"), Some("view"));
    assert_eq!(
        role("tests/Unit/Services/UserServiceTest.php"),
        Some("test")
    );
    assert_eq!(role("routes/web.php"), Some("route"));
    assert_eq!(role("config/app.php"), Some("config"));
}

#[test]
fn folder_only_and_suffix_only_and_agreement() {
    let c = Classifier::default();
    let both = c.classify("app/Services/UserService.php").unwrap();
    assert_eq!(both.confidence, Confidence::High);
    assert!(matches!(both.reason, Reason::Both(..)), "{both:?}");
    assert_eq!(both.anchor, "app/Services");
    let folder = c.classify("app/Http/Controllers/Auth/Login.php").unwrap();
    assert!(matches!(folder.reason, Reason::Folder(ref f) if f == "Controllers"));
    assert_eq!(folder.anchor, "app/Http/Controllers");
    let suffix = c.classify("app/Billing/InvoiceService.php").unwrap();
    assert_eq!(suffix.role.map(Role::name), Some("service"));
    assert!(matches!(suffix.reason, Reason::Name(ref n) if n == "*Service"));
    // ellentmondás: a név nyer, kisebb bizonyossággal
    let conflict = c.classify("app/Models/UserRepository.php").unwrap();
    assert_eq!(conflict.role.map(Role::name), Some("repository"));
    assert_eq!(conflict.confidence, Confidence::Medium);
    assert!(matches!(conflict.reason, Reason::Conflict(..)));
}

#[test]
fn a_pattern_suffix_in_a_layer_folder_keeps_the_layer() {
    let c = Classifier::default();
    let strategy = c
        .classify("app/Services/Payment/StripeStrategy.php")
        .unwrap();
    assert_eq!(strategy.role.map(Role::name), Some("service"));
    assert_eq!(strategy.pattern, Some("Strategy"));
    assert_eq!(
        strategy.pattern_example.as_deref(),
        Some("app/Services/Payment/*Strategy")
    );
    let repository = c.classify("app/Repositories/UserRepository.php").unwrap();
    assert_eq!(repository.pattern, Some("Repository"));
    assert_eq!(
        repository.pattern_example.as_deref(),
        Some("app/Repositories")
    );
}

#[test]
fn unknown_stays_unknown_and_non_code_is_skipped() {
    let c = Classifier::default();
    let support = c.classify("app/Support/Str.php").unwrap();
    assert_eq!(support.role, None);
    assert_eq!(support.reason, Reason::Unknown);
    assert_eq!(support.confidence, Confidence::Low);
    assert_eq!(role("src/main.ts"), None);
    assert_eq!(role("src/main.rs"), None);
    assert!(c.classify("README.md").is_none());
    assert!(c.classify("app/Http/Controllers/x.json").is_none());
    assert!(c.classify("Makefile").is_none());
}

#[test]
fn vue_and_typescript_names() {
    assert_eq!(role("src/components/UserCard.vue"), Some("component"));
    assert_eq!(confidence("src/components/UserCard.vue"), Confidence::High);
    assert_eq!(role("src/pages/Home.vue"), Some("page"));
    assert_eq!(role("src/views/HomeView.vue"), Some("page"));
    assert_eq!(role("src/composables/useAuth.ts"), Some("composable"));
    assert_eq!(confidence("src/composables/useAuth.ts"), Confidence::High);
    assert_eq!(role("src/lib/use-debounce.js"), Some("composable"));
    assert_eq!(role("src/stores/user.store.ts"), Some("store"));
    assert_eq!(role("src/stores/cart.ts"), Some("store"));
    assert_eq!(role("src/api/users.ts"), Some("api"));
    assert_eq!(role("src/router/index.ts"), Some("route"));
    assert_eq!(role("src/utils/format.spec.ts"), Some("test"));
    // a .vue fájl magában komponens, közepes bizonyossággal
    let app = Classifier::default().classify("src/App.vue").unwrap();
    assert_eq!(app.role.map(Role::name), Some("component"));
    assert_eq!(app.confidence, Confidence::Medium);
    assert_eq!(app.reason, Reason::Vue);
    // a PHP-s `views` (sablon) és a JS-es `views` (oldal) más
    assert_eq!(role("app/views/site/index.php"), Some("view"));
}

#[test]
fn swift_and_rust_names() {
    assert_eq!(role("App/Views/ContentView.swift"), Some("view"));
    assert_eq!(confidence("App/Views/ContentView.swift"), Confidence::High);
    assert_eq!(
        role("App/ViewModels/TodoViewModel.swift"),
        Some("viewmodel")
    );
    assert_eq!(role("App/Feature/TodoViewModel.swift"), Some("viewmodel"));
    assert_eq!(role("src/handlers/users.rs"), Some("handler"));
    assert_eq!(role("src/routes.rs"), Some("route"));
    assert_eq!(role("src/user_repo.rs"), Some("repository"));
    assert_eq!(role("src/db/mod.rs"), Some("repository"));
    assert_eq!(role("tests/api.rs"), Some("test"));
}

#[test]
fn ddd_context_folders_only_when_the_name_says_nothing() {
    assert_eq!(role("src/Domain/User/User.php"), Some("domain"));
    assert_eq!(
        role("src/Infrastructure/Persistence/Doctrine.php"),
        Some("infrastructure")
    );
    let repository = Classifier::default()
        .classify("src/Domain/User/UserRepository.php")
        .unwrap();
    assert_eq!(repository.role.map(Role::name), Some("repository"));
    assert_eq!(repository.confidence, Confidence::High);
    assert_eq!(
        role("src/Application/Services/Register.php"),
        Some("service")
    );
}

#[test]
fn role_names_accept_plurals_and_aliases() {
    assert_eq!(Role::parse("services").map(Role::name), Some("service"));
    assert_eq!(
        Role::parse("Repositories").map(Role::name),
        Some("repository")
    );
    assert_eq!(Role::parse("entity").map(Role::name), Some("model"));
    assert_eq!(Role::parse("hooks").map(Role::name), Some("composable"));
    assert_eq!(Role::parse("nope"), None);
}

#[test]
fn layers_file_maps_and_ignores_folders() {
    let text = "# saját nevek\nservice = app/Szolgaltatasok, ./app/Logic/\nignore-role = app/Legacy\nbogus = x\nno equals sign\n";
    let overrides = parse(text, None);
    assert_eq!(overrides.mappings.len(), 3);
    assert_eq!(overrides.warnings.len(), 2, "{:?}", overrides.warnings);
    let c = Classifier::new(overrides);
    let mapped = c.classify("app/Szolgaltatasok/Szamla.php").unwrap();
    assert_eq!(mapped.role.map(Role::name), Some("service"));
    assert_eq!(mapped.reason, Reason::Override);
    assert_eq!(mapped.confidence, Confidence::High);
    assert_eq!(
        c.role_of("app/Logic/Deep/Calc.php").map(Role::name),
        Some("service")
    );
    let ignored = c.classify("app/Legacy/OldController.php").unwrap();
    assert_eq!(ignored.role, None);
    assert_eq!(ignored.reason, Reason::Ignored);
    // szegmens-határ: az `app/Logicx` nem az `app/Logic` alatt van
    assert_eq!(c.role_of("app/Logicx/Calc.php"), None);
}

#[test]
fn layers_file_above_the_root_keeps_only_paths_inside_it() {
    let overrides = parse("service = sub/app/X, other/Y\nmodel = sub\n", Some("sub"));
    let prefixes: Vec<&str> = overrides
        .mappings
        .iter()
        .map(|m| m.prefix.as_str())
        .collect();
    assert_eq!(prefixes, ["app/X", ""]);
    assert_eq!(Overrides::default().lookup("a/b.php"), None);
}

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

const LARAVEL: &[&str] = &[
    "app/Http/Controllers/UserController.php",
    "app/Http/Controllers/OrderController.php",
    "app/Http/Requests/StoreUserRequest.php",
    "app/Services/UserService.php",
    "app/Services/Payment/StripeStrategy.php",
    "app/Repositories/UserRepository.php",
    "app/Repositories/UserRepositoryInterface.php",
    "app/Models/User.php",
    "app/Models/Order.php",
    "app/Support/Str.php",
    "database/factories/UserFactory.php",
    "README.md",
];

#[test]
fn aggregates_a_laravel_project() {
    let profile = ProjectProfile::aggregate(&entries(LARAVEL), vec!["laravel".into()], None, false);
    assert_eq!(profile.files, 11, "README.md is not a code file");
    assert_eq!(profile.architectures[0].id, "laravel");
    assert_eq!(profile.confidence(), Confidence::High);
    let controller = profile
        .layers
        .iter()
        .find(|l| l.layer.role.name() == "controller")
        .unwrap();
    assert_eq!(controller.files, 2);
    assert_eq!(controller.folders, [("app/Http/Controllers".to_owned(), 2)]);
    // előbb az architektúra rétegei, utánuk a minták; csoporton belül fájlszám szerint, azonos számnál név szerint
    let order: Vec<&str> = profile.layers.iter().map(|l| l.label.as_str()).collect();
    assert_eq!(
        order,
        [
            "controller",
            "model",
            "repository",
            "service",
            "request",
            "factory"
        ]
    );
    let patterns: Vec<(&str, usize)> = profile.patterns.iter().map(|p| (p.name, p.files)).collect();
    assert!(patterns.contains(&("Repository", 2)), "{patterns:?}");
    assert!(patterns.contains(&("Strategy", 1)), "{patterns:?}");
    assert!(patterns.contains(&("Factory", 1)), "{patterns:?}");
    assert_eq!(profile.unknown, [("app/Support".to_owned(), 1)]);
}

#[test]
fn architecture_needs_the_shape_without_a_framework() {
    let plain = ProjectProfile::aggregate(&entries(LARAVEL), Vec::new(), None, false);
    let ids: Vec<&str> = plain.architectures.iter().map(|a| a.id).collect();
    assert!(ids.contains(&"layered"), "{ids:?}");
    assert!(!ids.contains(&"laravel"), "{ids:?}");
    let vue = entries(&[
        "src/components/UserCard.vue",
        "src/pages/Home.vue",
        "src/stores/user.ts",
        "src/composables/useAuth.ts",
        "src/main.ts",
    ]);
    let with_framework = ProjectProfile::aggregate(&vue, vec!["vue".into()], None, false);
    assert_eq!(with_framework.architectures[0].id, "vue");
    assert_eq!(with_framework.confidence(), Confidence::High);
    let without = ProjectProfile::aggregate(&vue, Vec::new(), None, false);
    assert_eq!(without.architectures[0].id, "frontend");
    assert_eq!(without.confidence(), Confidence::Medium);
    let none = ProjectProfile::aggregate(
        &entries(&["lib/a.php", "lib/b.php"]),
        Vec::new(),
        None,
        false,
    );
    assert!(none.architectures.is_empty());
    assert_eq!(none.confidence(), Confidence::Low);
    let swift = entries(&[
        "App/Views/ListView.swift",
        "App/ViewModels/ListViewModel.swift",
    ]);
    let mvvm = ProjectProfile::aggregate(&swift, Vec::new(), None, false);
    assert_eq!(mvvm.architectures[0].id, "swiftui");
}

#[test]
fn compact_profile_is_short_and_bilingual() {
    let profile = ProjectProfile::aggregate(&entries(LARAVEL), vec!["laravel".into()], None, false);
    let en = render(&profile, Lang::En, false, Tip::Show);
    assert!(
        en.starts_with("profile: laravel (layered MVC) · confidence: high\nlayers (files):\n"),
        "{en}"
    );
    assert!(en.contains("unknown: app/Support (1 file)"), "{en}");
    assert!(
        en.contains("patterns: Repository (2: app/Repositories)"),
        "{en}"
    );
    assert!(en.lines().count() <= 15, "{en}");
    let hu = render(&profile, Lang::Hu, false, Tip::Hide);
    assert!(
        hu.starts_with("profil: laravel (layered MVC) · bizonyosság: magas\n"),
        "{hu}"
    );
    assert!(hu.contains("ismeretlen: app/Support (1 fájl)"), "{hu}");
    assert!(!hu.contains("💡"), "{hu}");
    let full = render(&profile, Lang::En, true, Tip::Hide);
    assert!(full.contains("frameworks: laravel"), "{full}");
    assert!(full.contains("    app/Http/Controllers  2"), "{full}");
    assert!(full.contains("(names only"), "{full}");
    let empty = ProjectProfile::aggregate(&[], Vec::new(), None, false);
    assert!(render(&empty, Lang::En, false, Tip::Show).starts_with("profile: no code files"));
}

fn graph(json: serde_json::Value) -> FileGraph {
    let mut base = serde_json::json!({
        "file": "x", "framework": null, "namespace": null, "imports": {}, "containers": [],
        "properties": [], "constants": [], "methods": [], "calls": [], "comments": [],
        "controlFlow": [], "strings": [], "writes": []
    });
    for (key, value) in json.as_object().unwrap() {
        base[key.as_str()] = value.clone();
    }
    serde_json::from_value(base).unwrap()
}

fn class(name: &str, extends: &[&str], implements: &[&str]) -> serde_json::Value {
    serde_json::json!({"type": "class", "name": name, "line": 1, "endLine": 9,
        "extends": extends, "implements": implements, "doc": null})
}

fn call(container: &str, resolved: &str) -> serde_json::Value {
    serde_json::json!({"container": container, "caller": "run", "line": 3, "type": "method",
        "target": "x", "resolved": resolved, "category": "internal"})
}

#[test]
fn graph_confirms_inheritance_and_interfaces() {
    let controller = graph(serde_json::json!({
        "namespace": "App\\Http\\Controllers",
        "containers": [class("UserController", &["Controller"], &[])]
    }));
    let repository = graph(serde_json::json!({
        "namespace": "App\\Repositories",
        "containers": [class("UserRepository", &["BaseRepository"], &[])]
    }));
    let base = graph(serde_json::json!({
        "namespace": "App\\Repositories",
        "containers": [class("BaseRepository", &[], &["UserRepositoryInterface"])]
    }));
    let plain = graph(serde_json::json!({"containers": [class("Thing", &[], &[])]}));
    let hierarchy = TypeHierarchy::from_graphs([&controller, &repository, &base, &plain]);
    let named = |n: &str| Role::named(n).unwrap();
    assert_eq!(
        confirm(named("controller"), &controller, &hierarchy),
        Some("extends *Controller")
    );
    assert_eq!(
        confirm(named("repository"), &repository, &hierarchy),
        Some("implements *RepositoryInterface")
    );
    assert_eq!(confirm(named("controller"), &plain, &hierarchy), None);
    assert_eq!(confirm(named("route"), &controller, &hierarchy), None);
    let store = graph(serde_json::json!({"calls": [call("store", "pinia::defineStore")]}));
    assert_eq!(
        confirm(named("store"), &store, &hierarchy),
        Some("defineStore")
    );
}

#[test]
fn flow_counts_cross_layer_calls_in_the_expected_direction() {
    let controller = graph(serde_json::json!({
        "namespace": "App\\Http\\Controllers",
        "containers": [class("UserController", &[], &[])],
        "calls": [call("UserController", "App\\Services\\UserService::find"),
                  call("UserController", "App\\Services\\UserService::all"),
                  call("UserController", "Illuminate\\Support\\Str::of")]
    }));
    let service = graph(serde_json::json!({
        "namespace": "App\\Services",
        "containers": [class("UserService", &[], &[])],
        "calls": [call("UserService", "App\\Repositories\\UserRepository::find"),
                  call("UserService", "App\\Services\\UserService::all")]
    }));
    let repository = graph(serde_json::json!({
        "namespace": "App\\Repositories",
        "containers": [class("UserRepository", &[], &[])],
        "calls": [call("UserRepository", "App\\Http\\Controllers\\UserController::back")]
    }));
    let page = graph(serde_json::json!({
        "module": "src/pages/Home.vue",
        "calls": [call("Home", "src/stores/user.ts::useUserStore")]
    }));
    let store = graph(serde_json::json!({"module": "src/stores/user.ts"}));
    let c = Classifier::default();
    let files = [
        ("app/Http/Controllers/UserController.php", &controller),
        ("app/Services/UserService.php", &service),
        ("app/Repositories/UserRepository.php", &repository),
        ("src/pages/Home.vue", &page),
        ("src/stores/user.ts", &store),
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
    // controller → service ×2, service → repository, repository → controller (vissza), page → store
    assert_eq!(flow.total, 5);
    assert_eq!(flow.expected, 4);
    assert_eq!(flow.percent(), 80);
    // a backend és a frontend rétegei külön lánc (nincs köztük hívás)
    let chain: Vec<Vec<&str>> = flow
        .chain
        .iter()
        .map(|group| group.iter().map(|l| l.role.name()).collect())
        .collect();
    assert_eq!(
        chain,
        [
            vec!["controller", "service", "repository"],
            vec!["page", "store"]
        ]
    );
    assert_eq!(flow.pairs[0].1, 2);
    let alone = [FlowFile {
        rel: "app/Models/User.php",
        layer: c.layer_of("app/Models/User.php"),
        graph: &store,
    }];
    assert!(compute(&alone).is_none());
}

#[test]
fn manifests_name_the_frameworks() {
    assert_eq!(
        js_frameworks(
            r#"{"dependencies": {"vue": "^3", "pinia": "^2"}, "devDependencies": {"vite": "5"}}"#
        ),
        ["vue"]
    );
    assert_eq!(
        js_frameworks(r#"{"dependencies": {"next": "14", "react": "18"}}"#),
        ["next", "react"]
    );
    assert!(js_frameworks("not json").is_empty());
    assert_eq!(
        rust_frameworks("[dependencies]\naxum = \"0.7\"\ntokio = { version = \"1\" }\n"),
        ["axum"]
    );
    assert_eq!(
        rust_frameworks("[dependencies.actix-web]\nversion = \"4\"\n"),
        ["actix-web"]
    );
    assert!(rust_frameworks("[dependencies]\naxumx = \"1\"\n").is_empty());
}
