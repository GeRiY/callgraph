//! A szimbólum-feloldó lánc egységtesztjei memóriabeli csomagtérképen: saját névtér vs. csomag-előtag (a
//! leghosszabb nyer), tranzitív csomag, homlokzat csak Laravellel, csomag-import (scope, al-út), és ami nem csomag
//! (relatív, alias, beépített modul); a hívásból képzett nyers minősítő.

use std::path::{Path, PathBuf};

use super::bare_import::package_name;
use super::{Attribution, Resolution, SymbolRef, resolve};
use crate::model::Call;
use crate::packages::model::{Ecosystem, Package};
use crate::packages::project::{EcosystemPackages, ProjectPackages};

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

/// PHP-rész: saját `App\`, a guzzle és a laravel közvetlenül, a symfony és a `GuzzleHttp\Psr7` tranzitívan.
fn php_part(laravel: bool) -> EcosystemPackages {
    let mut packages = vec![package("guzzlehttp/guzzle", "7.9.2", &["GuzzleHttp\\"])];
    if laravel {
        packages.push(package("laravel/framework", "11.31.0", &["Illuminate\\"]));
    }
    EcosystemPackages {
        root: root(),
        ecosystem: Ecosystem::Php,
        name: Some("acme/shop".to_owned()),
        packages,
        installed_only: vec![
            package(
                "symfony/http-foundation",
                "7.1.0",
                &["Symfony\\Component\\HttpFoundation\\"],
            ),
            package("guzzlehttp/psr7", "2.7.0", &["GuzzleHttp\\Psr7\\"]),
        ],
        own_namespaces: vec!["App\\".to_owned(), "Database\\Factories\\".to_owned()],
        lock: Some("composer.lock".to_owned()),
    }
}

fn node_part() -> EcosystemPackages {
    EcosystemPackages {
        root: root().join("web"),
        ecosystem: Ecosystem::Node,
        name: None,
        packages: vec![Package {
            ecosystem: Ecosystem::Node,
            ..package("axios", "1.7.2", &[])
        }],
        installed_only: vec![Package {
            ecosystem: Ecosystem::Node,
            ..package("@vueuse/core", "10.11.0", &[])
        }],
        own_namespaces: Vec::new(),
        lock: Some("package-lock.json".to_owned()),
    }
}

fn map(laravel: bool) -> ProjectPackages {
    ProjectPackages {
        root: root(),
        parts: vec![php_part(laravel), node_part()],
    }
}

fn php(qualifier: &'static str, alias: Option<&'static str>) -> SymbolRef<'static> {
    SymbolRef {
        path: Path::new("/proj/app/Services/Pay.php"),
        ecosystem: Ecosystem::Php,
        qualifier,
        alias,
    }
}

fn node(spec: &'static str) -> SymbolRef<'static> {
    SymbolRef {
        path: Path::new("/proj/web/src/api.ts"),
        ecosystem: Ecosystem::Node,
        qualifier: spec,
        alias: None,
    }
}

fn found(name: &str, version: Option<&str>) -> Resolution {
    Resolution::Package(Attribution {
        name: name.to_owned(),
        version: version.map(str::to_owned),
    })
}

#[test]
fn own_namespace_is_own_and_the_longest_package_prefix_wins() {
    let map = map(true);
    assert_eq!(
        resolve(&php("App\\Models\\User", None), &map),
        Resolution::Own
    );
    // a névtér kis- / nagybetű-független
    assert_eq!(
        resolve(&php("database\\factories\\UserFactory", None), &map),
        Resolution::Own
    );
    assert_eq!(
        resolve(&php("GuzzleHttp\\Client", None), &map),
        found("guzzlehttp/guzzle", Some("7.9.2"))
    );
    // `GuzzleHttp\Psr7\` hosszabb a `GuzzleHttp\`-nél: a tranzitív csomagé
    assert_eq!(
        resolve(&php("GuzzleHttp\\Psr7\\Request", None), &map),
        found("guzzlehttp/psr7", Some("2.7.0"))
    );
    // az előtag csak egész névtér-tagra illik
    assert_eq!(
        resolve(&php("GuzzleHttpExtra\\Thing", None), &map),
        Resolution::Unknown
    );
    assert_eq!(resolve(&php("strlen", None), &map), Resolution::Unknown);
}

#[test]
fn transitive_packages_resolve_by_their_own_name() {
    let map = map(true);
    assert_eq!(
        resolve(&php("Illuminate\\Support\\Facades\\Http", None), &map),
        found("laravel/framework", Some("11.31.0"))
    );
    assert_eq!(
        resolve(
            &php("Symfony\\Component\\HttpFoundation\\Request", None),
            &map
        ),
        found("symfony/http-foundation", Some("7.1.0"))
    );
}

#[test]
fn facade_needs_laravel_in_the_nearest_part() {
    // névtérben, import nélkül: az elemző a fájl névterébe oldja, a homlokzat-tábla mégis a Laravelé
    let unimported = php("App\\Services\\Http", Some("Http"));
    assert_eq!(
        resolve(&unimported, &map(true)),
        found("laravel/framework", Some("11.31.0"))
    );
    assert_eq!(resolve(&unimported, &map(false)), Resolution::Own);
    // nem homlokzat-név: a többi láncszem dönt
    assert_eq!(
        resolve(&php("App\\Services\\Money", Some("Money")), &map(true)),
        Resolution::Own
    );
    assert_eq!(
        resolve(&php("Route", Some("Route")), &map(false)),
        Resolution::Unknown
    );
}

#[test]
fn bare_imports_resolve_to_the_package_name_with_scope_and_without_sub_path() {
    let map = map(false);
    assert_eq!(resolve(&node("axios"), &map), found("axios", Some("1.7.2")));
    assert_eq!(
        resolve(&node("@vueuse/core/sub"), &map),
        found("@vueuse/core", Some("10.11.0"))
    );
    // nem deklarált csomag is (verzió nélkül)
    assert_eq!(
        resolve(&node("lodash/debounce"), &map),
        found("lodash", None)
    );
    assert_eq!(package_name("lodash/debounce"), Some("lodash"));
    assert_eq!(package_name("@scope/name/deep/path"), Some("@scope/name"));
}

#[test]
fn relative_alias_and_builtin_imports_are_not_packages() {
    let map = map(false);
    for spec in [
        "./local",
        "../up",
        "/abs/x",
        "@/components/A",
        "~/utils",
        "#internal",
        "$lib/x",
        "node:fs",
        "virtual:pwa",
        "@scope",
    ] {
        assert_eq!(package_name(spec), None, "{spec}");
        assert_eq!(resolve(&node(spec), &map), Resolution::Unknown, "{spec}");
    }
    for builtin in ["fs", "fs/promises", "path", "child_process"] {
        assert_eq!(
            resolve(&node(builtin), &map),
            Resolution::Unknown,
            "{builtin}"
        );
    }
    // Node-rész nélküli fájl: nincs csomag
    let outside = SymbolRef {
        path: Path::new("/proj/app/js/x.js"),
        ..node("axios")
    };
    assert_eq!(resolve(&outside, &map), Resolution::Unknown);
}

fn call(kind: &str, target: &str, resolved: Option<&str>, category: &str) -> Call {
    Call {
        container: None,
        caller: None,
        line: 1,
        end_line: None,
        kind: kind.into(),
        target: target.into(),
        resolved: resolved.map(str::to_owned),
        on: None,
        category: category.into(),
        args: None,
        handler: None,
        from: None,
    }
}

#[test]
fn symbol_of_a_call_takes_the_raw_qualifier() {
    let file = Path::new("/proj/app/Pay.php");
    let at = |call: &Call| {
        SymbolRef::of_call(file, call)
            .map(|sym| (sym.qualifier.to_owned(), sym.alias.map(str::to_owned)))
    };
    let typed = call(
        "method_call",
        "request",
        Some("GuzzleHttp\\Client::request"),
        "external_call",
    );
    assert_eq!(at(&typed), Some(("GuzzleHttp\\Client".to_owned(), None)));
    let created = call("new", "Client", Some("GuzzleHttp\\Client"), "framework_use");
    assert_eq!(
        at(&created),
        Some(("GuzzleHttp\\Client".to_owned(), Some("Client".to_owned())))
    );
    let facade = call(
        "static_call",
        "Http::get",
        Some("App\\Http::get"),
        "framework_use",
    );
    assert_eq!(
        at(&facade),
        Some(("App\\Http".to_owned(), Some("Http".to_owned())))
    );
    let global = call(
        "static_call",
        "Route::get",
        Some("Route::get"),
        "external_call",
    );
    assert_eq!(
        at(&global),
        Some(("Route".to_owned(), Some("Route".to_owned())))
    );
    // névtérben, saját osztály: nem homlokzat-jelölt
    let own = call(
        "static_call",
        "Money::of",
        Some("App\\Money::of"),
        "static_call",
    );
    assert_eq!(at(&own), Some(("App\\Money".to_owned(), None)));
    // feloldatlan fogadó: nincs minősítő
    assert_eq!(
        at(&call("method_call", "save", None, "external_call")),
        None
    );
    // JS: az import forrása
    let mut js = call("func_call", "get", Some("axios::get"), "http_call");
    js.from = Some("axios".to_owned());
    let sym = SymbolRef::of_call(Path::new("/proj/web/a.ts"), &js).unwrap();
    assert_eq!((sym.ecosystem, sym.qualifier), (Ecosystem::Node, "axios"));
    // nem PHP és nincs import-forrás: nincs minősítő
    let helper = call("func_call", "x", None, "global_helper");
    assert!(SymbolRef::of_call(Path::new("/proj/web/a.ts"), &helper).is_none());
}

#[test]
fn label_is_the_last_segment_unless_generic() {
    let label = |name: &str| {
        Attribution {
            name: name.to_owned(),
            version: None,
        }
        .label()
        .to_owned()
    };
    assert_eq!(label("guzzlehttp/guzzle"), "guzzle");
    assert_eq!(label("laravel/framework"), "laravel");
    assert_eq!(label("@vueuse/core"), "vueuse");
    assert_eq!(label("axios"), "axios");
    let stripe = Attribution {
        name: "stripe/stripe-php".to_owned(),
        version: None,
    };
    assert!(stripe.matches_term("stripe"));
    assert!(stripe.matches_term("stripe/stripe-php"));
    assert!(!stripe.matches_term("php"));
}
