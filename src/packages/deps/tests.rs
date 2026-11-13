//! A függőség-vizsgálat (`deps`) egységtesztjei memóriabeli csomagtérképen és gráfokon: a kivétel-szabályok
//! egyenként, a fölösleges / hiányzó / csak dev találatok, a kimenet (EN / HU, igazítás, `[mappa]` előtag, üres és
//! manifest nélküli eset).

use std::path::{Path, PathBuf};

use super::rules::{
    AnyOf, BuiltinModule, BundledFrontend, ComposerPlugin, CssOrAssetOnly, FrameworkAutoload,
    FrameworkBundled, NoNamespace, PackageRule, ToolingPackage, UserIgnore, WorkspacePackage,
    name_matches,
};
use super::{DepsQuery, Usage, is_dev_support_path, is_test_path};
use crate::i18n::Lang;
use crate::model::{Call, FileGraph};
use crate::packages::Packages;
use crate::packages::model::{Ecosystem, Package};
use crate::packages::project::{EcosystemPackages, ProjectPackages};

fn root() -> PathBuf {
    PathBuf::from("/proj")
}

fn php(name: &str, namespaces: &[&str]) -> Package {
    Package {
        name: name.to_owned(),
        ecosystem: Ecosystem::Php,
        declared: Some("*".to_owned()),
        installed: Some("1.0.0".to_owned()),
        dev: false,
        namespaces: namespaces.iter().map(|ns| (*ns).to_owned()).collect(),
        auto_discovered: false,
        plugin: false,
    }
}

fn node(name: &str) -> Package {
    Package {
        ecosystem: Ecosystem::Node,
        ..php(name, &[])
    }
}

fn dev(package: Package) -> Package {
    Package {
        dev: true,
        ..package
    }
}

fn part(dir: &Path, ecosystem: Ecosystem, packages: Vec<Package>) -> EcosystemPackages {
    EcosystemPackages {
        root: dir.to_path_buf(),
        ecosystem,
        name: None,
        packages,
        installed_only: Vec::new(),
        own_namespaces: if ecosystem == Ecosystem::Php {
            vec!["App\\".to_owned(), "Database\\".to_owned()]
        } else {
            Vec::new()
        },
        lock: Some("lock".to_owned()),
    }
}

/// Egy Laravel + Vite projekt a gyökérben (PHP és Node rész ugyanabban a mappában).
fn shop() -> Packages {
    let mut backend = part(
        &root(),
        Ecosystem::Php,
        vec![
            php("laravel/framework", &["Illuminate\\"]),
            php("guzzlehttp/guzzle", &["GuzzleHttp\\"]),
            php("stripe/stripe-php", &["Stripe\\"]),
            Package {
                auto_discovered: true,
                ..php("spatie/laravel-backup", &["Spatie\\Backup\\"])
            },
            Package {
                plugin: true,
                ..php("acme/installer", &["Acme\\Installer\\"])
            },
            php("acme/legacy", &[]),
            dev(php("laravel/pint", &[])),
            dev(php("fakerphp/faker", &["Faker\\"])),
            dev(php("phpunit/phpunit", &["PHPUnit\\"])),
        ],
    );
    backend.installed_only = vec![
        php(
            "symfony/http-foundation",
            &["Symfony\\Component\\HttpFoundation\\"],
        ),
        php("nette/utils", &["Nette\\"]),
    ];
    let frontend = part(
        &root(),
        Ecosystem::Node,
        vec![
            node("vue"),
            node("lodash"),
            node("@fontsource/inter"),
            node("my-theme"),
            dev(node("vite")),
            dev(node("axios")),
        ],
    );
    Packages::from_map(ProjectPackages {
        root: root(),
        parts: vec![backend, frontend],
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
fn php_call(kind: &str, target: &str, resolved: &str) -> Call {
    Call {
        container: Some("C".into()),
        caller: Some("run".into()),
        line: 1,
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
fn js_call(target: &str, from: &str) -> Call {
    Call {
        resolved: Some(format!("{from}::{target}")),
        category: "package_use".into(),
        from: Some(from.into()),
        ..php_call("func_call", target, "")
    }
}

fn file(path: &str, calls: Vec<Call>, imports: &[(&str, &str)]) -> (PathBuf, FileGraph) {
    let mut graph = graph(path);
    graph.calls = calls;
    for (alias, full) in imports {
        graph.imports.insert((*alias).into(), (*full).into());
    }
    (PathBuf::from(path), graph)
}

/// A projekt fájljai: guzzle és laravel a szolgáltatásban, egy tranzitív symfony (a laravel hozza) és nette (nem),
/// a faker az alkalmazás-kódban, a factoryban és (phpunit) a tesztben; JS: vue, nem deklarált dayjs, dev axios,
/// beépített `node:fs`, helyi import; CSS: a `my-theme` importja.
fn shop_files() -> Vec<(PathBuf, FileGraph)> {
    vec![
        file(
            "/proj/app/Services/PaymentGateway.php",
            vec![
                php_call("new", "Client", "GuzzleHttp\\Client"),
                php_call("static_call", "Str::slug", "Illuminate\\Support\\Str::slug"),
                php_call(
                    "method_call",
                    "send",
                    "Symfony\\Component\\HttpFoundation\\Response::send",
                ),
                php_call("method_call", "save", "App\\Models\\Order::save"),
            ],
            &[],
        ),
        file(
            "/proj/app/Services/Html.php",
            vec![php_call(
                "static_call",
                "Html::el",
                "Nette\\Utils\\Html::el",
            )],
            &[],
        ),
        file(
            "/proj/app/Support/DemoData.php",
            Vec::new(),
            &[("Factory", "Faker\\Factory")],
        ),
        file(
            "/proj/database/factories/UserFactory.php",
            vec![php_call("method_call", "name", "Faker\\Generator::name")],
            &[],
        ),
        file(
            "/proj/tests/Feature/PayTest.php",
            vec![php_call(
                "method_call",
                "assertTrue",
                "PHPUnit\\Framework\\TestCase::assertTrue",
            )],
            &[],
        ),
        file(
            "/proj/resources/js/app.ts",
            vec![
                js_call("createApp", "vue"),
                js_call("dayjs", "dayjs"),
                js_call("axios.get", "axios"),
                js_call("readFileSync", "node:fs"),
                js_call("join", "path"),
            ],
            &[("helper", "resources/js/lib/helper.ts")],
        ),
        file(
            "/proj/resources/js/utils/date.ts",
            Vec::new(),
            &[("dayjs", "dayjs")],
        ),
        file(
            "/proj/resources/css/app.css",
            Vec::new(),
            &[
                ("theme", "my-theme/dist/theme.css"),
                ("print.css", "print.css"),
            ],
        ),
    ]
}

fn render(
    packages: &Packages,
    files: &[(PathBuf, FileGraph)],
    ignore: &[&str],
    lang: Lang,
) -> String {
    let root = root();
    let ignore = ignore.iter().map(|p| (*p).to_owned()).collect();
    DepsQuery::new(packages, &root, ignore).render(
        files.iter().map(|(path, graph)| (path.as_path(), graph)),
        lang,
    )
}

#[test]
fn reports_unused_missing_and_dev_only_grouped_and_aligned() {
    assert_eq!(
        render(&shop(), &shop_files(), &[], Lang::En),
        "unused?   stripe/stripe-php  no reference in code\n\
         unused?   lodash             no reference in code\n\
         missing   nette/utils        used in app/Services/Html.php, not in composer.json (transitive)\n\
         missing   dayjs              imported in resources/js/app.ts (+1), not in package.json\n\
         dev-only  fakerphp/faker     used in app/Support/DemoData.php\n"
    );
}

#[test]
fn hungarian_labels_and_reasons() {
    let text = render(&shop(), &shop_files(), &[], Lang::Hu);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            "fölösleges?  stripe/stripe-php  nincs hivatkozás a kódban",
            "fölösleges?  lodash             nincs hivatkozás a kódban",
            "hiányzik     nette/utils        használva: app/Services/Html.php, nincs a composer.json-ban (tranzitív)",
            "hiányzik     dayjs              importálva: resources/js/app.ts (+1), nincs a package.json-ban",
            "csak dev     fakerphp/faker     használva: app/Support/DemoData.php",
        ]
    );
}

#[test]
fn deps_ignore_silences_names_and_globs() {
    assert_eq!(
        render(
            &shop(),
            &shop_files(),
            &[
                "stripe/*",
                "lodash",
                "nette/utils",
                "dayjs",
                "fakerphp/faker"
            ],
            Lang::En
        ),
        "no dependency issues found\n"
    );
    assert_eq!(
        render(
            &shop(),
            &shop_files(),
            &[
                "stripe/*",
                "lodash",
                "nette/utils",
                "dayjs",
                "fakerphp/faker"
            ],
            Lang::Hu
        ),
        "nincs függőség-probléma\n"
    );
}

#[test]
fn no_manifest_is_one_line_without_walking_files() {
    let empty = Packages::default();
    let root = root();
    let query = DepsQuery::new(&empty, &root, Vec::new());
    assert!(!query.needs_files());
    assert_eq!(
        query.render(std::iter::empty(), Lang::En),
        "deps: no composer.json / package.json in the project\n"
    );
    assert!(DepsQuery::new(&shop(), &root, Vec::new()).needs_files());
}

/// Több csomag-mappa: a sor elején `[mappa]`; a workspace a gyökérből felhúzott függőséget nem jelzi hiányzónak,
/// a gyökérét nem fölöslegesnek.
#[test]
fn several_folders_get_a_prefix_and_hoisted_packages_count_for_the_root() {
    let backend = part(
        &root(),
        Ecosystem::Php,
        vec![php("stripe/stripe-php", &["Stripe\\"])],
    );
    let shared = part(&root(), Ecosystem::Node, vec![node("vue")]);
    let mut web = part(&root().join("web"), Ecosystem::Node, vec![node("lodash")]);
    web.name = Some("@acme/web".to_owned());
    let packages = Packages::from_map(ProjectPackages {
        root: root(),
        parts: vec![backend, shared, web],
    });
    let files = [
        file(
            "/proj/web/src/main.ts",
            vec![js_call("createApp", "vue")],
            &[],
        ),
        file(
            "/proj/web/src/self.ts",
            vec![js_call("x", "@acme/web")],
            &[],
        ),
    ];
    assert_eq!(
        render(&packages, &files, &[], Lang::En),
        "[.] unused?   stripe/stripe-php  no reference in code\n\
         [web] unused?   lodash             no reference in code\n"
    );
}

/// A PHP-csomag `use`-ja (hívás nélkül) is használat; a teszt- és a factory-fájl nem alkalmazás-kód.
#[test]
fn dev_only_needs_app_code_outside_tests_and_factories() {
    let packages = Packages::from_map(ProjectPackages {
        root: root(),
        parts: vec![part(
            &root(),
            Ecosystem::Php,
            vec![dev(php("mockery/mockery", &["Mockery\\"]))],
        )],
    });
    let in_tests = [
        file(
            "/proj/tests/Unit/OrderTest.php",
            Vec::new(),
            &[("MockInterface", "Mockery\\MockInterface")],
        ),
        file(
            "/proj/database/seeders/DemoSeeder.php",
            Vec::new(),
            &[("M", "Mockery\\Mock")],
        ),
    ];
    assert_eq!(
        render(&packages, &in_tests, &[], Lang::En),
        "no dependency issues found\n"
    );
    let in_app = [
        file("/proj/app/A.php", Vec::new(), &[("Mock", "Mockery\\Mock")]),
        file("/proj/app/B.php", Vec::new(), &[("Mock", "Mockery\\Mock")]),
    ];
    assert_eq!(
        render(&packages, &in_app, &[], Lang::En),
        "dev-only  mockery/mockery  used in app/A.php (+1)\n"
    );
}

/// Bundler nélküli Node-rész: a dev-függőség az alkalmazás-kódban jelez; bundlerrel nem.
#[test]
fn node_dev_only_only_without_a_bundler() {
    let files = [file(
        "/proj/src/server.ts",
        vec![js_call("z.object", "zod")],
        &[],
    )];
    let plain = Packages::from_map(ProjectPackages {
        root: root(),
        parts: vec![part(&root(), Ecosystem::Node, vec![dev(node("zod"))])],
    });
    assert_eq!(
        render(&plain, &files, &[], Lang::En),
        "dev-only  zod  used in src/server.ts\n"
    );
    let bundled = Packages::from_map(ProjectPackages {
        root: root(),
        parts: vec![part(
            &root(),
            Ecosystem::Node,
            vec![dev(node("zod")), dev(node("vite"))],
        )],
    });
    assert_eq!(
        render(&bundled, &files, &[], Lang::En),
        "no dependency issues found\n"
    );
}

#[test]
fn test_and_dev_support_paths() {
    for test in [
        "tests/Feature/PayTest.php",
        "app/test/a.php",
        "src/__tests__/a.ts",
        "spec/a.rb",
        "src/a.test.ts",
        "src/a.spec.js",
        "app/OrderTest.php",
    ] {
        assert!(is_test_path(test), "{test}");
    }
    for app in ["app/Services/Testing.php", "src/latest.ts", "app/Tests.php"] {
        assert!(!is_test_path(app), "{app}");
    }
    for support in [
        "database/factories/UserFactory.php",
        "database/seeders/DatabaseSeeder.php",
        "stubs/model.stub.php",
        "vite.config.ts",
        "tailwind.config.js",
    ] {
        assert!(is_dev_support_path(support), "{support}");
    }
    assert!(!is_dev_support_path("app/Database/Factory.php"));
}

#[test]
fn usage_counts_calls_and_refs_but_not_css() {
    let usage = Usage {
        css: 2,
        ..Usage::default()
    };
    assert!(!usage.used());
    assert!(
        Usage {
            refs: 1,
            ..Usage::default()
        }
        .used()
    );
}

// --- a szabályok egyenként ---

fn none() -> Usage {
    Usage::default()
}

#[test]
fn name_patterns_are_exact_or_prefix_globs() {
    assert!(name_matches("phpstan/*", "phpstan/phpstan"));
    assert!(name_matches("eslint*", "eslint-plugin-vue"));
    assert!(name_matches("eslint*", "eslint"));
    assert!(name_matches("Laravel/Pint", "laravel/pint"));
    assert!(!name_matches("vite", "vitest"));
    assert!(!name_matches("@types/*", "@typescript-eslint/parser"));
}

#[test]
fn tooling_package_rule() {
    for name in [
        "laravel/pint",
        "phpstan/phpstan",
        "pestphp/pest",
        "fakerphp/faker",
    ] {
        assert!(ToolingPackage.matches(&php(name, &[]), &none()), "{name}");
    }
    for name in [
        "typescript",
        "@types/node",
        "eslint-plugin-vue",
        "prettier",
        "@vitejs/plugin-vue",
    ] {
        assert!(ToolingPackage.matches(&node(name), &none()), "{name}");
    }
    assert!(!ToolingPackage.matches(&php("stripe/stripe-php", &[]), &none()));
    // az ökoszisztéma számít: npm-ben nincs `laravel/pint`
    assert!(!ToolingPackage.matches(&node("laravel/pint"), &none()));
    assert!(!ToolingPackage.matches(&node("lodash"), &none()));
}

#[test]
fn framework_autoload_rule_uses_the_name_and_the_lock_flag() {
    assert!(FrameworkAutoload.matches(&php("laravel/sanctum", &[]), &none()));
    assert!(FrameworkAutoload.matches(&php("rakutentech/laravel-request-docs", &[]), &none()));
    assert!(FrameworkAutoload.matches(&php("tightenco/ziggy", &[]), &none()));
    let discovered = Package {
        auto_discovered: true,
        ..php("maatwebsite/excel", &[])
    };
    assert!(FrameworkAutoload.matches(&discovered, &none()));
    assert!(!FrameworkAutoload.matches(&php("maatwebsite/excel", &[]), &none()));
    assert!(!FrameworkAutoload.matches(&node("laravel-vite-plugin"), &none()));
}

#[test]
fn composer_plugin_rule_uses_the_lock_flag() {
    let plugin = Package {
        plugin: true,
        ..php("pestphp/pest-plugin", &[])
    };
    assert!(ComposerPlugin.matches(&plugin, &none()));
    assert!(ComposerPlugin.matches(&php("php-http/discovery", &[]), &none()));
    assert!(!ComposerPlugin.matches(&php("guzzlehttp/guzzle", &[]), &none()));
}

#[test]
fn css_or_asset_only_rule() {
    assert!(CssOrAssetOnly.matches(&node("@fontsource/inter"), &none()));
    assert!(CssOrAssetOnly.matches(&node("primeicons"), &none()));
    assert!(CssOrAssetOnly.matches(&node("bootstrap"), &none()));
    let from_css = Usage {
        css: 1,
        ..Usage::default()
    };
    assert!(CssOrAssetOnly.matches(&node("my-theme"), &from_css));
    assert!(!CssOrAssetOnly.matches(&node("my-theme"), &none()));
    assert!(!CssOrAssetOnly.matches(&php("bootstrap", &[]), &none()));
}

#[test]
fn no_namespace_rule() {
    assert!(NoNamespace.matches(&php("acme/legacy", &[]), &none()));
    assert!(!NoNamespace.matches(&php("acme/modern", &["Acme\\"]), &none()));
    assert!(!NoNamespace.matches(&node("lodash"), &none()));
}

#[test]
fn user_ignore_rule() {
    let rule = UserIgnore::new(vec!["spatie/*".to_owned(), "lodash".to_owned()]);
    assert!(rule.matches(&php("spatie/laravel-backup", &[]), &none()));
    assert!(rule.matches(&node("lodash"), &none()));
    assert!(!rule.matches(&node("lodash-es"), &none()));
    assert!(!UserIgnore::new(Vec::new()).matches(&node("lodash"), &none()));
}

#[test]
fn builtin_module_rule() {
    for name in ["fs", "path", "node:fs", "node:test"] {
        assert!(BuiltinModule.matches(&node(name), &none()), "{name}");
    }
    for name in ["php", "ext-json", "lib-icu"] {
        assert!(BuiltinModule.matches(&php(name, &[]), &none()), "{name}");
    }
    assert!(!BuiltinModule.matches(&node("dayjs"), &none()));
    assert!(!BuiltinModule.matches(&php("nette/utils", &[]), &none()));
}

#[test]
fn framework_bundled_rule_needs_the_declared_framework() {
    let laravel = part(
        &root(),
        Ecosystem::Php,
        vec![php("laravel/framework", &["Illuminate\\"])],
    );
    let rule = FrameworkBundled::of(&laravel);
    for name in [
        "symfony/console",
        "illuminate/support",
        "psr/log",
        "nesbot/carbon",
        "league/flysystem",
    ] {
        assert!(rule.matches(&php(name, &[]), &none()), "{name}");
    }
    assert!(!rule.matches(&php("nette/utils", &[]), &none()));
    let bare = part(&root(), Ecosystem::Php, Vec::new());
    assert!(!FrameworkBundled::of(&bare).matches(&php("symfony/console", &[]), &none()));
}

#[test]
fn workspace_package_rule() {
    let mut web = part(&root().join("web"), Ecosystem::Node, Vec::new());
    web.name = Some("@acme/web".to_owned());
    let map = ProjectPackages {
        root: root(),
        parts: vec![web],
    };
    let rule = WorkspacePackage::of(&map);
    assert!(rule.matches(&node("@acme/web"), &none()));
    assert!(!rule.matches(&node("@acme/api"), &none()));
}

#[test]
fn bundled_frontend_rule() {
    let vite = part(&root(), Ecosystem::Node, vec![dev(node("vite"))]);
    assert!(BundledFrontend::of(&vite).matches(&node("axios"), &none()));
    let plain = part(&root(), Ecosystem::Node, vec![node("express")]);
    assert!(!BundledFrontend::of(&plain).matches(&node("axios"), &none()));
}

#[test]
fn any_of_names_the_first_matching_rule() {
    let rules: [&dyn PackageRule; 2] = [&ToolingPackage, &NoNamespace];
    let any = AnyOf(&rules);
    assert_eq!(
        any.first_match(&php("laravel/pint", &[]), &none()),
        Some("tooling")
    );
    assert_eq!(
        any.first_match(&php("acme/legacy", &[]), &none()),
        Some("no-namespace")
    );
    assert_eq!(
        any.first_match(&php("acme/modern", &["Acme\\"]), &none()),
        None
    );
    assert!(any.matches(&php("acme/legacy", &[]), &none()));
}
