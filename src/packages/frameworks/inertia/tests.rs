//! Az Inertia-kötések egységtesztjei memóriabeli fájlrendszeren (`ProjectFiles`): a page-gyökér felismerése (glob,
//! `resolvePageComponent`, sablon, tartalék mappa), a page-név ↔ fájl, a render-hívások és argumentumaik (a tárolt
//! `args`-ból és a forrásból), a `rendered by:` szöveg és az `inertia` nézet sorai.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::super::files::ProjectFiles;
use super::pages::{self, PageRoot};
use super::render::{self, RenderKind};
use super::{Inertia, PageLink};
use crate::i18n::Lang;
use crate::model::{Call, FileGraph, Property};
use crate::packages::{Ecosystem, EcosystemPackages, Package, Packages, ProjectPackages};

/// Memóriabeli fájlrendszer: a gyökérhez relatív út → szöveg.
struct MemFiles(BTreeMap<PathBuf, String>);

impl MemFiles {
    fn new(files: &[(&str, &str)]) -> Self {
        Self(
            files
                .iter()
                .map(|(rel, text)| (root().join(rel), (*text).to_owned()))
                .collect(),
        )
    }
}

impl ProjectFiles for MemFiles {
    fn read(&self, path: &Path) -> Option<String> {
        self.0.get(path).cloned()
    }

    fn files_under(&self, dir: &Path) -> Vec<PathBuf> {
        self.0
            .keys()
            .filter(|path| path.starts_with(dir))
            .cloned()
            .collect()
    }
}

fn root() -> PathBuf {
    PathBuf::from("/p")
}

fn vue_root(dir: &str) -> PageRoot {
    PageRoot {
        dir: dir.into(),
        extensions: vec!["vue".into()],
    }
}

fn call(kind: &str, target: &str, resolved: Option<&str>, line: u32, args: &[&str]) -> Call {
    Call {
        container: Some("BoardController".into()),
        caller: Some("show".into()),
        line,
        end_line: None,
        kind: kind.into(),
        target: target.into(),
        resolved: resolved.map(str::to_owned),
        on: None,
        category: "framework_use".into(),
        args: (!args.is_empty()).then(|| args.iter().map(|a| (*a).to_owned()).collect()),
        handler: None,
        from: None,
    }
}

fn render(line: u32, args: &[&str]) -> Call {
    call(
        "static_call",
        "Inertia::render",
        Some("Inertia\\Inertia::render"),
        line,
        args,
    )
}

fn graph(file: &str, calls: Vec<Call>) -> FileGraph {
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
        calls,
        comments: Vec::new(),
        control_flow: Vec::new(),
        strings: Vec::new(),
        writes: Vec::new(),
        sfc: None,
        binary: false,
        parse_errors: Vec::new(),
    }
}

fn prop(name: &str) -> Property {
    Property {
        container: Some("Board".into()),
        name: name.into(),
        line: 3,
        type_hint: None,
        visibility: "public".into(),
        is_static: false,
        readonly: false,
        doc: None,
        js: true,
    }
}

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

const APP_JS: &str = "import { createInertiaApp } from '@inertiajs/vue3'\n\
createInertiaApp({\n  title: (title) => `${title} - Board`,\n  \
resolve: (name) => resolvePageComponent(`./Pages/${name}.vue`, import.meta.glob('./Pages/**/*.vue')),\n})\n";

#[test]
fn page_root_comes_from_the_glob_or_the_template_of_the_entry() {
    let found = |text: &str| pages::from_entry(text, "resources/js");
    assert_eq!(found(APP_JS), Some(vue_root("resources/js/Pages")));
    let typed = "createInertiaApp({ resolve: (n) => { const pages = import.meta.glob<DefineComponent>('./pages/**/*.{vue,tsx}', { eager: true }); return pages[n] } })";
    assert_eq!(
        found(typed),
        Some(PageRoot {
            dir: "resources/js/pages".into(),
            extensions: s(&["vue", "tsx"]),
        })
    );
    let aliased = "createInertiaApp({ resolve: (n) => resolvePageComponent(n, import.meta.glob('@/Pages/**/*.vue')) })";
    assert_eq!(found(aliased), Some(vue_root("resources/js/Pages")));
    let absolute = "createInertiaApp({ resolve: (n) => import.meta.glob(['/resources/js/Screens/**/*.vue']) })";
    assert_eq!(found(absolute), Some(vue_root("resources/js/Screens")));
    // csak sablon (a kiterjesztés nélkül: az alapértelmezettek)
    let template = "createInertiaApp({ resolve: name => require(`./Pages/${name}`) })";
    assert_eq!(
        found(template),
        Some(PageRoot {
            dir: "resources/js/Pages".into(),
            extensions: s(&["vue", "tsx", "jsx", "svelte"]),
        })
    );
    // nem Inertia-belépő; Inertia-belépő felismerhető mappa nélkül
    assert_eq!(found("import.meta.glob('./Pages/**/*.vue')"), None);
    assert_eq!(found("createInertiaApp({ resolve: pages })"), None);
}

#[test]
fn page_root_detection_falls_back_to_the_pages_folder() {
    let entry = MemFiles::new(&[
        ("resources/js/app.js", APP_JS),
        ("resources/js/Pages/Board.vue", "<template/>"),
    ]);
    assert_eq!(
        pages::detect(&root(), &entry),
        Some(vue_root("resources/js/Pages"))
    );
    let fallback = MemFiles::new(&[
        ("resources/js/app.js", "import './bootstrap'\n"),
        ("resources/js/pages/Home.vue", "<template/>"),
    ]);
    let found = pages::detect(&root(), &fallback).unwrap();
    assert_eq!(found.dir, "resources/js/pages");
    assert_eq!(found.extensions, s(&["vue", "tsx", "jsx", "svelte"]));
    assert_eq!(
        pages::detect(&root(), &MemFiles::new(&[("app/X.php", "<?php")])),
        None
    );
}

#[test]
fn page_names_map_to_files_and_back() {
    let pages = PageRoot {
        dir: "resources/js/Pages".into(),
        extensions: s(&["vue", "tsx"]),
    };
    assert_eq!(
        pages.candidates("Users/Index"),
        s(&[
            "resources/js/Pages/Users/Index.vue",
            "resources/js/Pages/Users/Index.tsx"
        ])
    );
    assert!(pages.candidates("../secret").is_empty());
    assert!(pages.candidates("/abs").is_empty());
    assert!(pages.candidates("").is_empty());
    assert_eq!(
        pages.page_name("resources/js/Pages/Users/Index.vue"),
        Some("Users/Index".into())
    );
    assert_eq!(pages.page_name("resources/js/Pages/Users/Index.ts"), None);
    assert_eq!(pages.page_name("resources/js/Components/Nav.vue"), None);
    assert_eq!(
        pages.short("resources/js/Pages/Users/Index.vue"),
        "Pages/Users/Index.vue"
    );

    let inertia = Inertia::new(
        &root(),
        pages,
        s(&[
            "resources/js/Pages/Board.vue",
            "resources/js/Pages/Users/Index.tsx",
        ]),
    );
    assert_eq!(
        inertia.resolve("Users/Index"),
        Some(PageLink {
            rel: "resources/js/Pages/Users/Index.tsx".into(),
            short: "Pages/Users/Index.tsx".into(),
            exists: true,
        })
    );
    assert_eq!(
        inertia.resolve("Gone"),
        Some(PageLink {
            rel: "resources/js/Pages/Gone.vue".into(),
            short: "Pages/Gone.vue".into(),
            exists: false,
        })
    );
    assert_eq!(
        inertia.page_name("/p/resources/js/Pages/Board.vue"),
        Some("Board".into())
    );
    assert!(inertia.is_page("resources/js/Pages/Board.vue"));
    assert!(!inertia.is_page("/elsewhere/resources/js/Pages/Board.vue"));
}

#[test]
fn render_calls_are_recognised_by_their_target() {
    assert_eq!(RenderKind::of(&render(1, &[])), Some(RenderKind::Render));
    let unimported = call("static_call", "Inertia::render", None, 1, &[]);
    assert_eq!(RenderKind::of(&unimported), Some(RenderKind::Render));
    let helper = call("func_call", "inertia", None, 1, &[]);
    assert_eq!(RenderKind::of(&helper), Some(RenderKind::Helper));
    let route = call(
        "static_call",
        "Route::inertia",
        Some("Illuminate\\Support\\Facades\\Route::inertia"),
        1,
        &[],
    );
    assert_eq!(RenderKind::of(&route), Some(RenderKind::Route));
    let view = call("func_call", "view", None, 1, &[]);
    assert_eq!(RenderKind::of(&view), None);
    let other = call("method_call", "render", Some("App\\View::render"), 1, &[]);
    assert_eq!(RenderKind::of(&other), None);
}

#[test]
fn page_name_is_a_plain_literal_and_props_are_array_keys() {
    let args = s(&["'Board'", "['title' => $t, \"board\" => $board->load('x')]"]);
    assert_eq!(
        render::page_of(RenderKind::Render, &args),
        Some("Board".into())
    );
    assert_eq!(
        render::props_of(RenderKind::Render, &args),
        Some(s(&["title", "board"]))
    );
    // nevesített argumentum
    let named = s(&["component: 'Users/Index'", "props: ['users' => $u]"]);
    assert_eq!(
        render::page_of(RenderKind::Render, &named),
        Some("Users/Index".into())
    );
    assert_eq!(
        render::props_of(RenderKind::Render, &named),
        Some(s(&["users"]))
    );
    // dinamikus név: kimarad
    assert_eq!(render::page_of(RenderKind::Render, &s(&["$page"])), None);
    assert_eq!(
        render::page_of(RenderKind::Render, &s(&["\"Users/$x\""])),
        None
    );
    assert_eq!(render::page_of(RenderKind::Render, &s(&["'Bo…"])), None);
    // Route::inertia: a második a page, a harmadik a props
    let route = s(&["'/about'", "'About'", "compact('team')"]);
    assert_eq!(
        render::page_of(RenderKind::Route, &route),
        Some("About".into())
    );
    assert_eq!(
        render::props_of(RenderKind::Route, &route),
        Some(s(&["team"]))
    );
    // props nélkül: üres; változó / spread / levágott: nem statikus
    assert_eq!(
        render::props_of(RenderKind::Render, &s(&["'X'"])),
        Some(Vec::new())
    );
    assert_eq!(
        render::props_of(RenderKind::Render, &s(&["'X'", "$props"])),
        None
    );
    assert_eq!(render::prop_keys("[...$base, 'a' => 1]"), None);
    assert_eq!(
        render::prop_keys("['a' => 1, 'b' => fn () => 2,]"),
        Some(s(&["a", "b"]))
    );
    assert_eq!(render::prop_keys("['a' => 1, 'b…"), None);
    assert_eq!(render::prop_keys("array('a' => [1, 2])"), Some(s(&["a"])));
}

#[test]
fn source_args_read_the_whole_multiline_call() {
    let text = "<?php\nclass BoardController\n{\n    public function show()\n    {\n        return Inertia::render('Board', [\n            'title' => 'Hello, (world)',\n            'board' => $board,\n            'honorary' => $this->honorary(),\n        ]);\n    }\n}\n";
    let args = render::source_args(text, 6, RenderKind::Render).unwrap();
    assert_eq!(args.len(), 2, "{args:?}");
    assert_eq!(args[0], "'Board'");
    assert_eq!(
        render::prop_keys(&args[1]),
        Some(s(&["title", "board", "honorary"]))
    );
    assert_eq!(
        render::source_args("<?php\nreturn inertia('Home');\n", 2, RenderKind::Helper),
        Some(s(&["'Home'"]))
    );
    // nincs ilyen hívás a sorban / lezáratlan
    assert_eq!(render::source_args(text, 5, RenderKind::Render), None);
    assert_eq!(
        render::source_args("<?php\nInertia::render('X', [\n", 2, RenderKind::Render),
        None
    );
}

/// Egy megjegyzésben álló idézőjel nem nyit stringet, és a megjegyzés nem kerül az argumentumokba.
#[test]
fn source_args_skip_php_comments() {
    let text = "<?php\nreturn Inertia::render('Home', [\n    'title' => 'Főoldal',\n    // a „hero\" sosem marad üres\n    /* régi: 'x' => 1, */\n    # 'y' => 2,\n    'intro' => $intro,\n]);\n";
    let args = render::source_args(text, 2, RenderKind::Render).unwrap();
    assert_eq!(args.len(), 2, "{args:?}");
    assert_eq!(args[0], "'Home'");
    assert_eq!(render::prop_keys(&args[1]), Some(s(&["title", "intro"])));
}

#[test]
fn links_sites_and_rendered_by_come_from_calls_and_sources() {
    let mut inertia = Inertia::new(
        &root(),
        vue_root("resources/js/Pages"),
        s(&["resources/js/Pages/Board.vue", "resources/js/Pages/Old.vue"]),
    );
    // a teljes gráf hívása (tárolt `args`): a nyíl
    let full = render(6, &["'Board'", "['title' => $t]"]);
    assert_eq!(
        inertia.link(&full).map(|link| (link.short, link.exists)),
        Some(("Pages/Board.vue".to_owned(), true))
    );
    assert_eq!(
        inertia
            .link(&render(6, &["'Missing'"]))
            .map(|link| link.exists),
        Some(false)
    );
    assert_eq!(inertia.link(&render(6, &["$name"])), None);

    // a szűkített tények (args nélkül): a forrásból
    let source = "<?php\nclass BoardController\n{\n    public function show()\n    {\n        return Inertia::render('Board', ['title' => $t, 'extra' => 1]);\n    }\n    public function edit()\n    {\n        return Inertia::render('Board');\n    }\n}\n";
    let files = MemFiles::new(&[("app/Http/Controllers/BoardController.php", source)]);
    let controller = graph(
        "/p/app/Http/Controllers/BoardController.php",
        vec![render(6, &[]), {
            let mut edit = render(10, &[]);
            edit.caller = Some("edit".into());
            edit
        }],
    );
    let mut page = graph("/p/resources/js/Pages/Board.vue", Vec::new());
    page.properties = vec![prop("title")];
    let controller_path = root().join("app/Http/Controllers/BoardController.php");
    let page_path = root().join("resources/js/Pages/Board.vue");
    inertia.scan(
        [
            (controller_path.as_path(), &controller),
            (page_path.as_path(), &page),
        ],
        &files,
    );
    let sites = inertia.sites();
    assert_eq!(sites.len(), 2, "{sites:?}");
    assert_eq!(sites[0].file, "app/Http/Controllers/BoardController.php");
    assert_eq!(sites[0].props, Some(s(&["title", "extra"])));
    assert_eq!(sites[1].props, Some(Vec::new()));
    assert_eq!(
        inertia.rendered_by("/p/resources/js/Pages/Board.vue"),
        Some("BoardController::show L6 (+1)".into())
    );
    assert_eq!(inertia.rendered_by("/p/resources/js/Pages/Old.vue"), None);
    assert_eq!(
        inertia.rendered_by("/p/app/Http/Controllers/BoardController.php"),
        None
    );
}

#[test]
fn view_lines_mark_missing_pages_undeclared_props_and_unrendered_pages() {
    let mut inertia = Inertia::fixture(
        &root(),
        "resources/js/Pages",
        &[
            "resources/js/Pages/Board.vue",
            "resources/js/Pages/Old.vue",
            "resources/js/Pages/About.vue",
        ],
        &[
            (
                "app/Http/Controllers/BoardController.php",
                "BoardController",
                "show",
                23,
                "Board",
            ),
            (
                "app/Http/Controllers/BoardController.php",
                "BoardController",
                "gone",
                40,
                "Gone",
            ),
            ("routes/web.php", "", "__top__", 8, "About"),
        ],
    );
    inertia.sites[0].props = Some(s(&["title", "board", "honorary"]));
    inertia.declared.insert(
        "resources/js/Pages/Board.vue".into(),
        s(&["title", "board"]),
    );
    let scope = s(&["app/Http/Controllers/BoardController.php", "routes/web.php"]);
    let lines = inertia.view_lines(&scope, Lang::En);
    assert_eq!(
        lines,
        s(&[
            "Pages/About.vue  ← routes/web.php L8",
            "Pages/Board.vue  ← BoardController::show L23 · props: title, board, honorary  ! props not declared: honorary",
            "Pages/Gone.vue  ← BoardController::gone L40  ! page file missing",
            "unrendered: Pages/Old.vue",
        ])
    );
    // a hatókörön kívüli render-hely kimarad; a page-fájl a hatókörben behozza
    let only_page = inertia.view_lines(&s(&["resources/js/Pages/Board.vue"]), Lang::Hu);
    assert_eq!(only_page.len(), 2, "{only_page:?}");
    assert!(
        only_page[0].ends_with("! nem deklarált props: honorary"),
        "{only_page:?}"
    );
    assert_eq!(only_page[1], "nem renderelt: Pages/Old.vue");
}

#[test]
fn the_strategy_needs_the_inertia_package_and_a_page_root() {
    let package = |name: &str, ecosystem| Package {
        name: name.into(),
        ecosystem,
        declared: Some("*".into()),
        installed: None,
        dev: false,
        namespaces: Vec::new(),
        auto_discovered: false,
        plugin: false,
    };
    let packages = |names: &[(&str, Ecosystem)]| {
        Packages::from_map(ProjectPackages {
            root: root(),
            parts: vec![EcosystemPackages {
                root: root(),
                ecosystem: Ecosystem::Php,
                name: None,
                packages: names.iter().map(|&(n, e)| package(n, e)).collect(),
                installed_only: Vec::new(),
                own_namespaces: Vec::new(),
                lock: None,
            }],
        })
    };
    let files = MemFiles::new(&[
        ("resources/js/app.js", APP_JS),
        ("resources/js/Pages/Board.vue", "<template/>"),
        ("resources/js/Pages/notes.md", "# x"),
    ]);
    let laravel = packages(&[("laravel/framework", Ecosystem::Php)]);
    assert!(Inertia::detect(&root(), &laravel, &files).is_none());
    let with_inertia = packages(&[
        ("laravel/framework", Ecosystem::Php),
        ("inertiajs/inertia-laravel", Ecosystem::Php),
    ]);
    let inertia = Inertia::detect(&root(), &with_inertia, &files).unwrap();
    assert_eq!(
        inertia.files.iter().cloned().collect::<Vec<_>>(),
        s(&["resources/js/Pages/Board.vue"])
    );
    assert!(with_inertia.frameworks().contains(&"inertia"));
    // page-gyökér nélkül sincs stratégia
    let empty = MemFiles::new(&[]);
    assert!(Inertia::detect(&root(), &with_inertia, &empty).is_none());
}
