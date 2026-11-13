//! A csomag-használat (`uses`) egységtesztjei memóriabeli csomagtérképen és gráfokon: a csomag kiválasztása
//! (pontos név, címke, többértelmű, ismeretlen, manifest nélkül), az összesítés (darabszám, első fájl, `+N`, a
//! sorrend, a 20 soros korlát), a `calls` lista, a hívás nélküli import (`(type)`) és a nem deklarált JS-csomag.

use std::path::{Path, PathBuf};

use super::{Target, UsageQuery};
use crate::i18n::Lang;
use crate::model::{Call, FileGraph};
use crate::packages::Packages;
use crate::packages::model::{Ecosystem, Package};
use crate::packages::project::{EcosystemPackages, ProjectPackages};
use crate::packages::resolve::Attribution;

fn root() -> PathBuf {
    PathBuf::from("/proj")
}

fn package(name: &str, version: &str, namespaces: &[&str]) -> Package {
    Package {
        name: name.to_owned(),
        ecosystem: Ecosystem::Php,
        declared: Some("*".to_owned()),
        installed: Some(version.to_owned()),
        dev: false,
        namespaces: namespaces.iter().map(|ns| (*ns).to_owned()).collect(),
    }
}

fn node(name: &str, version: &str) -> Package {
    Package {
        ecosystem: Ecosystem::Node,
        ..package(name, version, &[])
    }
}

/// PHP: guzzle, psr7 és sanctum közvetlenül, a symfony tranzitívan, saját `App\`; Node (`web/`): axios, vue.
fn packages() -> Packages {
    let php = EcosystemPackages {
        root: root(),
        ecosystem: Ecosystem::Php,
        name: Some("acme/shop".to_owned()),
        packages: vec![
            package("guzzlehttp/guzzle", "7.9.2", &["GuzzleHttp\\"]),
            package("guzzlehttp/psr7", "2.7.0", &["GuzzleHttp\\Psr7\\"]),
            package("laravel/sanctum", "4.0.3", &["Laravel\\Sanctum\\"]),
        ],
        installed_only: vec![package(
            "symfony/http-foundation",
            "7.1.0",
            &["Symfony\\Component\\HttpFoundation\\"],
        )],
        own_namespaces: vec!["App\\".to_owned()],
        lock: Some("composer.lock".to_owned()),
    };
    let web = EcosystemPackages {
        root: root().join("web"),
        ecosystem: Ecosystem::Node,
        name: None,
        packages: vec![node("axios", "1.7.2"), node("vue", "3.4.0")],
        installed_only: Vec::new(),
        own_namespaces: Vec::new(),
        lock: Some("package-lock.json".to_owned()),
    };
    Packages::from_map(ProjectPackages {
        root: root(),
        parts: vec![php, web],
    })
}

fn graph(file: &str) -> FileGraph {
    FileGraph {
        file: file.into(),
        module: None,
        framework: None,
        namespace: None,
        imports: Default::default(),
        containers: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        methods: Vec::new(),
        calls: Vec::new(),
        comments: Vec::new(),
        control_flow: Vec::new(),
        strings: Vec::new(),
        writes: Vec::new(),
        sfc: None,
        binary: false,
        parse_errors: Vec::new(),
    }
}

/// PHP-hívás: `kind`, a forrásbeli cél, a feloldott cél.
fn php_call(line: u32, kind: &str, target: &str, resolved: &str) -> Call {
    Call {
        container: Some("C".into()),
        caller: Some("run".into()),
        line,
        end_line: None,
        kind: kind.into(),
        target: target.into(),
        resolved: Some(resolved.into()),
        on: None,
        category: "external_call".into(),
        args: None,
        handler: None,
        from: None,
    }
}

/// JS-hívás egy nem helyi importból.
fn js_call(line: u32, target: &str, from: &str) -> Call {
    Call {
        kind: "func_call".into(),
        resolved: Some(format!("{from}::{target}")),
        category: "package_use".into(),
        from: Some(from.into()),
        ..php_call(line, "func_call", target, "")
    }
}

/// A kis projekt: két szolgáltatás guzzle-hívásokkal és egy csak típusként használt kivétellel, egy saját
/// osztályt hívó kontroller, és egy TS-fájl axios-szal.
fn project() -> Vec<(PathBuf, FileGraph)> {
    let mut gateway = graph("/proj/app/Services/PaymentGateway.php");
    gateway
        .imports
        .insert("Client".into(), "GuzzleHttp\\Client".into());
    gateway.imports.insert(
        "RequestException".into(),
        "GuzzleHttp\\Exception\\RequestException".into(),
    );
    gateway.calls = vec![
        php_call(14, "method_call", "request", "GuzzleHttp\\Client::request"),
        php_call(15, "method_call", "request", "GuzzleHttp\\Client::request"),
        php_call(24, "new", "Client", "GuzzleHttp\\Client"),
        php_call(30, "method_call", "save", "App\\Models\\Order::save"),
    ];
    let mut dhl = graph("/proj/app/Services/Shipping/Dhl.php");
    dhl.imports
        .insert("Client".into(), "GuzzleHttp\\Client".into());
    dhl.calls = vec![
        php_call(10, "new", "Client", "GuzzleHttp\\Client"),
        php_call(11, "method_call", "get", "GuzzleHttp\\Client::get"),
    ];
    let mut controller = graph("/proj/app/Http/Controllers/PayController.php");
    controller.imports.insert(
        "PaymentGateway".into(),
        "App\\Services\\PaymentGateway".into(),
    );
    controller.calls = vec![php_call(
        9,
        "method_call",
        "charge",
        "App\\Services\\PaymentGateway::charge",
    )];
    let mut api = graph("/proj/web/src/api.ts");
    api.imports.insert("axios".into(), "axios".into());
    api.imports.insert("ref".into(), "vue::ref".into());
    api.imports
        .insert("helper".into(), "src/lib/helper.ts".into());
    api.calls = vec![
        js_call(3, "axios.get", "axios"),
        js_call(4, "axios.post", "axios"),
        js_call(7, "useMouse", "@vueuse/core"),
    ];
    vec![
        (PathBuf::from(&gateway.file), gateway),
        (PathBuf::from(&dhl.file), dhl),
        (PathBuf::from(&controller.file), controller),
        (PathBuf::from(&api.file), api),
    ]
}

fn files(project: &[(PathBuf, FileGraph)]) -> impl Iterator<Item = (&Path, &FileGraph)> {
    project.iter().map(|(path, graph)| (path.as_path(), graph))
}

fn render(term: &str, calls: bool, lang: Lang) -> String {
    let packages = packages();
    let project = project();
    let root = root();
    UsageQuery::new(&packages, &root, term).render(files(&project), calls, lang)
}

#[test]
fn summary_counts_calls_symbols_files_and_type_only_imports() {
    let text = render("guzzle", false, Lang::En);
    assert_eq!(
        text,
        "guzzlehttp/guzzle 7.9.2 · 5 calls · 1 ref · 2 files\n\
         \x20 Client::request   2  app/Services/PaymentGateway.php\n\
         \x20 new Client        2  app/Services/PaymentGateway.php +1\n\
         \x20 Client::get       1  app/Services/Shipping/Dhl.php\n\
         \x20 RequestException  1  (type) app/Services/PaymentGateway.php\n"
    );
    // a teljes név és a kis- / nagybetű sem számít; magyarul ugyanaz a szerkezet
    assert_eq!(render("GuzzleHttp/Guzzle", false, Lang::En), text);
    let hu = render("guzzle", false, Lang::Hu);
    assert!(
        hu.starts_with("guzzlehttp/guzzle 7.9.2 · 5 hívás · 1 hivatkozás · 2 fájl\n"),
        "{hu}"
    );
    assert!(
        hu.contains("(típus) app/Services/PaymentGateway.php"),
        "{hu}"
    );
}

#[test]
fn calls_lists_every_call_line_per_file() {
    assert_eq!(
        render("guzzle", true, Lang::En),
        "guzzlehttp/guzzle 7.9.2 · 5 calls · 1 ref · 2 files\n\
         app/Services/PaymentGateway.php\n\
         \x20 L14 Client::request\n\
         \x20 L15 Client::request\n\
         \x20 L24 new Client\n\
         \x20 (type) RequestException\n\
         app/Services/Shipping/Dhl.php\n\
         \x20 L10 new Client\n\
         \x20 L11 Client::get\n"
    );
}

#[test]
fn javascript_imports_and_an_undeclared_package() {
    // a default import hívással: nincs `(type)` sor; a `vue::ref` import hívás nélkül: típus-hivatkozás
    assert_eq!(
        render("axios", false, Lang::En),
        "axios 1.7.2 · 2 calls · 1 file\n\
         \x20 axios.get   1  web/src/api.ts\n\
         \x20 axios.post  1  web/src/api.ts\n"
    );
    assert_eq!(
        render("vue", false, Lang::En),
        "vue 3.4.0 · 0 calls · 1 ref · 1 file\n\
         \x20 ref  1  (type) web/src/api.ts\n"
    );
    // nem deklarált csomag: a hívásokból (a helyi import feloldott útja — `src/lib/helper.ts` — nem csomag)
    assert_eq!(
        render("vueuse", false, Lang::En),
        "@vueuse/core · 1 call · 1 file\n\
         \x20 useMouse  1  web/src/api.ts\n"
    );
    assert!(render("src", false, Lang::En).starts_with("unknown package: src"));
}

#[test]
fn ambiguous_unknown_unused_and_no_manifest() {
    assert_eq!(
        render("guzzlehttp", false, Lang::En),
        "ambiguous package name: guzzlehttp → guzzlehttp/guzzle, guzzlehttp/psr7 (give the full name)\n"
    );
    assert_eq!(
        render("guzle", false, Lang::En),
        "unknown package: guzle (not in composer.json / package.json or a lock, and no call uses it); closest: guzzlehttp/guzzle\n"
    );
    assert!(
        render("guzle", false, Lang::Hu).starts_with("ismeretlen csomag: guzle ("),
        "hu"
    );
    assert_eq!(
        render("sanctum", false, Lang::En),
        "laravel/sanctum 4.0.3: not used in the code (no call or import found)\n"
    );
    // a csak a lockban szereplő csomag is választható (és itt nem használt)
    assert_eq!(
        render("http-foundation", true, Lang::En),
        "symfony/http-foundation 7.1.0: not used in the code (no call or import found)\n"
    );
    let empty = Packages::default();
    let root = root();
    let query = UsageQuery::new(&empty, &root, "guzzle");
    assert_eq!(query.target, Target::NoManifest);
    assert!(!query.needs_files());
    assert_eq!(
        query.render(std::iter::empty(), false, Lang::En),
        "unknown package: guzzle (no composer.json / package.json in the project)\n"
    );
}

#[test]
fn the_choice_prefers_the_label_and_direct_dependencies() {
    let packages = packages();
    let root = root();
    let target = |term: &str| UsageQuery::new(&packages, &root, term).target;
    assert_eq!(
        target("guzzle"),
        Target::Known(Attribution {
            name: "guzzlehttp/guzzle".into(),
            version: Some("7.9.2".into()),
        })
    );
    assert_eq!(
        target("psr7"),
        Target::Known(Attribution {
            name: "guzzlehttp/psr7".into(),
            version: Some("2.7.0".into()),
        })
    );
    assert_eq!(
        target("guzzlehttp"),
        Target::Ambiguous(vec!["guzzlehttp/guzzle".into(), "guzzlehttp/psr7".into()])
    );
    assert_eq!(target("vueuse"), Target::Search);
    assert!(!UsageQuery::new(&packages, &root, "guzzlehttp").needs_files());
    assert!(UsageQuery::new(&packages, &root, "vueuse").needs_files());
}

#[test]
fn the_summary_stops_at_twenty_symbols() {
    let packages = packages();
    let root = root();
    let mut big = graph("/proj/app/Big.php");
    big.calls = (0..25)
        .map(|i| {
            php_call(
                i + 1,
                "method_call",
                &format!("m{i:02}"),
                &format!("GuzzleHttp\\Client::m{i:02}"),
            )
        })
        .collect();
    let project = [(PathBuf::from(&big.file), big)];
    let text = UsageQuery::new(&packages, &root, "guzzle").render(files(&project), false, Lang::En);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1 + 20 + 1, "{text}");
    assert_eq!(lines[0], "guzzlehttp/guzzle 7.9.2 · 25 calls · 1 file");
    assert_eq!(lines[1], "  Client::m00  1  app/Big.php");
    assert_eq!(lines[21], "  +5 more");
}
