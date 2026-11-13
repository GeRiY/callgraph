//! A projekt-profil (`callgraph profile`) és a réteg-kapcsolók (`--layer`, `--layers`, `--by-layer`,
//! `callers --by-layer`, `.callgraph-layers`) egy kis Laravel-szerű PHP- és egy Vue / TS-projekten. A kapcsolók
//! nélkül a meglévő kimenetek változatlanok: a címkék nélküli szöveg bájtra azonos a kapcsoló nélkülivel.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("callgraph-profile-{id}-{}-{n}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let project = Self(root.canonicalize().unwrap());
        for (path, body) in files {
            project.write(path, body);
        }
        project
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn command(&self, args: &[&str], lang: &str) -> Output {
        self.command_env(args, lang, &[])
    }

    /// Mint a `command`, további környezeti változókkal (pl. `CALLGRAPH_NO_GRAPH=1`).
    fn command_env(&self, args: &[&str], lang: &str, env: &[(&str, &str)]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", lang)
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
            .envs(env.iter().copied())
            .output()
            .expect("callgraph runs")
    }

    fn run(&self, args: &[&str]) -> String {
        self.run_in(args, "en")
    }

    fn run_in(&self, args: &[&str], lang: &str) -> String {
        let out = self.command(args, lang);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const CONTROLLER: &str =
    "<?php\nnamespace App\\Http\\Controllers;\n\nabstract class Controller\n{\n}\n";

const USER_CONTROLLER: &str = "<?php\nnamespace App\\Http\\Controllers;\n\nuse App\\Repositories\\UserRepositoryInterface;\nuse App\\Services\\UserService;\n\nclass UserController extends Controller\n{\n    private UserService $service;\n    private UserRepositoryInterface $repo;\n\n    public function show(int $id): array\n    {\n        return $this->service->find($id);\n    }\n\n    public function index(): array\n    {\n        return $this->repo->find(1);\n    }\n}\n";

const USER_SERVICE: &str = "<?php\nnamespace App\\Services;\n\nuse App\\Repositories\\UserRepositoryInterface;\n\nclass UserService\n{\n    private UserRepositoryInterface $repo;\n\n    public function find(int $id): array\n    {\n        return $this->repo->find($id);\n    }\n}\n";

const REPOSITORY_INTERFACE: &str = "<?php\nnamespace App\\Repositories;\n\ninterface UserRepositoryInterface\n{\n    public function find(int $id): array;\n}\n";

const REPOSITORY: &str = "<?php\nnamespace App\\Repositories;\n\nuse App\\Models\\User;\n\nclass UserRepository implements UserRepositoryInterface\n{\n    private User $model;\n\n    public function find(int $id): array\n    {\n        return $this->model->load($id);\n    }\n}\n";

const MODEL: &str = "<?php\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass User extends Model\n{\n    public function load(int $id): array\n    {\n        return [];\n    }\n}\n";

const SUPPORT: &str = "<?php\nnamespace App\\Support;\n\nclass Str\n{\n    public static function of(string $s): string\n    {\n        return $s;\n    }\n}\n";

fn laravel() -> Project {
    Project::new(&[
        (
            "composer.json",
            "{\"require\": {\"laravel/framework\": \"^11\"}}\n",
        ),
        ("app/Http/Controllers/Controller.php", CONTROLLER),
        ("app/Http/Controllers/UserController.php", USER_CONTROLLER),
        ("app/Services/UserService.php", USER_SERVICE),
        (
            "app/Repositories/UserRepositoryInterface.php",
            REPOSITORY_INTERFACE,
        ),
        ("app/Repositories/UserRepository.php", REPOSITORY),
        ("app/Models/User.php", MODEL),
        ("app/Support/Str.php", SUPPORT),
    ])
}

/// A `  [szerep]` címkék nélkül (a kapcsoló nélküli kimenettel összevetéshez).
fn untagged(text: &str) -> String {
    ["controller", "service", "repository", "model"]
        .iter()
        .fold(text.to_owned(), |acc, role| {
            acc.replace(&format!("  [{role}]"), "")
        })
}

fn line_with<'a>(text: &'a str, start: &str) -> &'a str {
    text.lines()
        .find(|line| line.starts_with(start))
        .unwrap_or_else(|| panic!("no line starting with {start:?}:\n{text}"))
}

#[test]
fn profile_recognises_a_laravel_project() {
    let project = laravel();
    let out = project.run(&["profile"]);
    // a headline után a kiemelt csomagok sora (lock nélkül a deklarált tartomány, jelölve)
    assert!(
        out.starts_with(
            "profile: laravel (layered MVC) · confidence: high\nstack:   laravel/framework ^11 (range)\nlayers (files):\n"
        ),
        "{out}"
    );
    let controller = line_with(&out, "  controller ");
    assert!(controller.contains("app/Http/Controllers"), "{out}");
    assert!(controller.contains("   2"), "{out}");
    assert!(controller.contains("(extends *Controller: 1)"), "{out}");
    let repository = line_with(&out, "  repository ");
    assert!(repository.contains("app/Repositories"), "{out}");
    assert!(
        repository.contains("(implements *RepositoryInterface: 1)"),
        "{out}"
    );
    assert!(
        line_with(&out, "  model ").contains("(extends Model / ActiveRecord: 1)"),
        "{out}"
    );
    assert!(
        line_with(&out, "  service ").contains("app/Services"),
        "{out}"
    );
    assert!(
        out.contains("patterns: Repository (2: app/Repositories)"),
        "{out}"
    );
    let flow = line_with(&out, "flow: ");
    assert!(
        flow.starts_with("flow: controller → service → repository → model"),
        "{out}"
    );
    assert!(flow.contains("checked: 100%"), "{out}");
    assert!(out.contains("unknown: app/Support (1 file)"), "{out}");
    assert!(out.lines().count() <= 15, "{out}");

    let full = project.run(&["profile", "--full"]);
    assert!(full.contains("frameworks: laravel"), "{full}");
    assert!(full.contains("    app/Http/Controllers  2"), "{full}");
    assert!(full.contains("  controller → service  1"), "{full}");

    let hu = project.run_in(&["profile"], "hu");
    assert!(
        hu.starts_with("profil: laravel (layered MVC) · bizonyosság: magas\n"),
        "{hu}"
    );
    assert!(hu.contains("ismeretlen: app/Support (1 fájl)"), "{hu}");
}

#[test]
fn layer_filter_keeps_only_that_layer() {
    let project = laravel();
    let services = project.run(&["show", "app", "--layer", "service", "--full"]);
    assert!(services.contains("UserService.php"), "{services}");
    assert!(!services.contains("UserController.php"), "{services}");
    let two = project.run(&["show", "app", "--layer", "controller,model", "--full"]);
    assert!(two.contains("UserController.php"), "{two}");
    assert!(two.contains("User.php"), "{two}");
    assert!(!two.contains("UserService.php"), "{two}");
    // a megnevezett fájl a szűrőtől függetlenül marad
    let named = project.run(&["show", "app/Support/Str.php", "--layer", "service"]);
    assert!(named.contains("Str.php"), "{named}");
    // a --grep is csak a réteg fájljaiban keres
    let grep = project.run(&[
        "show",
        "app",
        "--grep",
        "UserRepositoryInterface::find",
        "--layer",
        "service",
    ]);
    assert!(grep.contains("UserService"), "{grep}");
    assert!(!grep.contains("UserController"), "{grep}");
    let wrong = project.command(&["show", "app", "--layer", "nope"], "en");
    assert!(!wrong.status.success());
    let stderr = String::from_utf8_lossy(&wrong.stderr);
    assert!(
        stderr.contains("Unknown layer: nope (known: route, "),
        "{stderr}"
    );
}

#[test]
fn layers_flag_tags_and_groups_without_changing_the_rest() {
    let project = laravel();
    let plain = project.run(&["show", "app", "--full"]);
    let tagged = project.run(&["show", "app", "--full", "--layers"]);
    assert!(!plain.contains("  [controller]"), "{plain}");
    assert!(
        tagged.contains("UserController.php  NS:App\\Http\\Controllers  [controller]\n"),
        "{tagged}"
    );
    assert!(tagged.contains("Str.php  NS:App\\Support\n"), "{tagged}");
    assert_eq!(untagged(&tagged), plain);

    let summary = project.run(&["show", "app", "--format", "summary"]);
    assert!(!summary.contains("[controller]"), "{summary}");
    let grouped = project.run(&["show", "app", "--format", "summary", "--layers"]);
    assert!(grouped.starts_with("[controller]  2 files\n"), "{grouped}");
    assert!(grouped.contains("\n[service]  1 file\n"), "{grouped}");
    assert!(
        grouped.contains("\n[unknown]  1 file\nStr.php"),
        "{grouped}"
    );
    let order: Vec<usize> = [
        "[controller]",
        "[service]",
        "[repository]",
        "[model]",
        "[unknown]",
    ]
    .iter()
    .map(|h| grouped.find(h).unwrap_or_else(|| panic!("{h}: {grouped}")))
    .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{grouped}");
    // ugyanazok a fájl-sorok, csak csoportosítva
    let mut a: Vec<&str> = summary.lines().collect();
    let mut b: Vec<&str> = grouped.lines().filter(|l| !l.starts_with('[')).collect();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b);
}

#[test]
fn by_layer_orders_callers_from_the_closest_layer() {
    let project = laravel();
    let args = [
        "show",
        "app",
        "--grep",
        "UserRepositoryInterface::find",
        "--depth",
        "2",
    ];
    let plain = project.run(&args);
    let mut with_flag = args.to_vec();
    with_flag.push("--by-layer");
    let layered = project.run(&with_flag);
    let at = |text: &str, needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle}: {text}"))
    };
    // alapból fájl szerint (a controller elöl), --by-layer-rel a hívotthoz közelebbi service elöl
    assert!(
        at(&plain, "UserController::index()") < at(&plain, "UserService::find()"),
        "{plain}"
    );
    assert!(
        at(&layered, "UserService::find()") < at(&layered, "UserController::index()"),
        "{layered}"
    );
    assert!(!plain.contains("  [service]"), "{plain}");
    // szintenként rétegcsoportok, mindegyik előtt fejléc (a sorok végén nincs címke)
    assert!(layered.contains("\n  [service]  1 call\n"), "{layered}");
    assert!(layered.contains("\n  [controller]  1 call\n"), "{layered}");
    assert!(
        layered.contains("\n  [controller]  1 caller\n"),
        "{layered}"
    );
    let mut a: Vec<String> = plain.lines().map(str::to_owned).collect();
    let mut b: Vec<String> = layered
        .lines()
        .filter(|line| !line.starts_with("  ["))
        .map(str::to_owned)
        .collect();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "same lines, other order");

    let rows = |text: &str| -> Vec<serde_json::Value> {
        text.lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };
    let default_rows = rows(&project.run(&["callers", "UserRepositoryInterface::find"]));
    assert!(default_rows.iter().all(|row| row.get("layer").is_none()));
    let layered_rows =
        rows(&project.run(&["callers", "UserRepositoryInterface::find", "--by-layer"]));
    assert_eq!(layered_rows.len(), default_rows.len());
    assert_eq!(layered_rows[0]["layer"], "service", "{layered_rows:?}");
    assert_eq!(
        layered_rows.last().unwrap()["layer"],
        "controller",
        "{layered_rows:?}"
    );
}

#[test]
fn callgraph_layers_file_maps_own_folder_names() {
    let project = laravel();
    project.write(
        ".callgraph-layers",
        "# saját nevek\nservice = app/Support\nignore-role = app/Models\n",
    );
    let out = project.run(&["profile"]);
    assert!(!out.contains("unknown: app/Support"), "{out}");
    assert!(line_with(&out, "  service ").contains("(+1 more)"), "{out}");
    assert!(!out.contains("  model "), "{out}");
    assert!(out.contains("unknown: app/Models (1 file)"), "{out}");
    let full = project.run(&["profile", "--full"]);
    assert!(full.contains(".callgraph-layers: "), "{full}");
    assert!(full.contains("    app/Support  1"), "{full}");
    let services = project.run(&["show", "app", "--layer", "service", "--full"]);
    assert!(services.contains("Str.php"), "{services}");
    let models = project.run(&["show", "app", "--layer", "model", "--full"]);
    assert!(!models.contains("FILE  "), "{models}");
}

#[test]
fn profile_recognises_a_vue_project() {
    let project = Project::new(&[
        (
            "package.json",
            "{\"name\": \"shop\", \"dependencies\": {\"vue\": \"^3.4\", \"pinia\": \"^2\"}}\n",
        ),
        (
            "src/components/UserCard.vue",
            "<template>\n  <div class=\"card\">{{ name }}</div>\n</template>\n\n<script setup>\ndefineProps({ name: String })\n</script>\n",
        ),
        (
            "src/pages/Home.vue",
            "<template>\n  <UserCard :name=\"store.name\" />\n</template>\n\n<script setup>\nimport UserCard from '../components/UserCard.vue'\nimport { useUserStore } from '../stores/user'\n\nconst store = useUserStore()\n</script>\n",
        ),
        (
            "src/stores/user.ts",
            "import { defineStore } from 'pinia'\n\nexport const useUserStore = defineStore('user', {\n  state: () => ({ name: 'Ada' }),\n})\n",
        ),
        (
            "src/composables/useAuth.ts",
            "export function useAuth() {\n  return { loggedIn: false }\n}\n",
        ),
        (
            "src/main.ts",
            "import { createApp } from 'vue'\nimport Home from './pages/Home.vue'\n\ncreateApp(Home).mount('#app')\n",
        ),
    ]);
    let out = project.run(&["profile"]);
    assert!(
        out.starts_with(
            "profile: Vue frontend (pages / components / composables / stores) · confidence: high\n"
        ),
        "{out}"
    );
    for role in ["page", "component", "composable", "store"] {
        line_with(&out, &format!("  {role} "));
    }
    assert!(
        line_with(&out, "  store ").contains("(defineStore: 1)"),
        "{out}"
    );
    assert!(out.contains("unknown: src (1 file)"), "{out}");
    let stores = project.run(&["show", "src", "--layer", "store", "--full"]);
    assert!(stores.contains("user.ts"), "{stores}");
    assert!(!stores.contains("Home.vue"), "{stores}");
}

/// A stack-sor a lock pontos verziójával, és a `--packages` lista csomag-gyökerenként (prod, utána dev).
#[test]
fn profile_stack_line_and_packages_list_from_the_locks() {
    let project = Project::new(&[
        (
            "composer.json",
            "{\"require\": {\"php\": \"^8.2\", \"laravel/framework\": \"^11.0\"}, \"require-dev\": {\"phpunit/phpunit\": \"^10.5\"}}\n",
        ),
        (
            "composer.lock",
            "{\"packages\": [{\"name\": \"laravel/framework\", \"version\": \"v11.31.0\"}, {\"name\": \"symfony/console\", \"version\": \"v7.1.0\"}], \"packages-dev\": [{\"name\": \"phpunit/phpunit\", \"version\": \"10.5.1\"}]}\n",
        ),
        (
            "frontend/package.json",
            "{\"dependencies\": {\"vue\": \"^3.4.0\"}, \"devDependencies\": {\"vite\": \"^5.0.0\"}}\n",
        ),
        (
            "frontend/yarn.lock",
            "# yarn lockfile v1\n\nvue@^3.4.0:\n  version \"3.4.21\"\n\nvite@^5.0.0:\n  version \"5.1.4\"\n",
        ),
        ("app/Http/Controllers/Controller.php", CONTROLLER),
        ("app/Models/User.php", MODEL),
    ]);
    let out = project.run(&["profile"]);
    assert_eq!(
        line_with(&out, "stack:"),
        "stack:   laravel/framework 11.31.0 · phpunit/phpunit 10.5.1 · vue 3.4.21 · vite 5.1.4   (composer.lock, yarn.lock)",
        "{out}"
    );
    assert!(!out.contains("packages ["), "{out}");

    let listed = project.run(&["profile", "--packages"]);
    let at = listed
        .find("packages [php] . · composer.lock\n")
        .unwrap_or_else(|| panic!("{listed}"));
    assert!(
        listed[at..].starts_with(
            "packages [php] . · composer.lock\n  laravel/framework  11.31.0 (^11.0)\n  phpunit/phpunit    10.5.1 (^10.5)  [dev]\npackages [node] frontend · yarn.lock\n  vue   3.4.21 (^3.4.0)\n  vite  5.1.4 (^5.0.0)  [dev]\n"
        ),
        "{listed}"
    );
    assert!(!listed.contains("symfony/console"), "{listed}");
    let hu = project.run_in(&["profile", "--packages"], "hu");
    assert!(
        hu.contains("csomagok [node] frontend · yarn.lock\n"),
        "{hu}"
    );
}

#[test]
fn profile_without_code_files_says_so() {
    let project = Project::new(&[("docs/readme.md", "# x\n")]);
    let out = project.run(&["profile"]);
    assert!(out.starts_with("profile: no code files"), "{out}");
}

const CART_CONTROLLER: &str = "<?php\nnamespace app\\modules\\shop\\controllers;\n\nclass CartController extends Controller\n{\n    public function actionIndex()\n    {\n        return [];\n    }\n}\n";

const ORDER_CONTROLLER: &str = "<?php\nnamespace app\\modules\\shop\\controllers;\n\nclass OrderController extends Controller\n{\n    public function actionIndex()\n    {\n        return [];\n    }\n}\n";

const CART_MODEL: &str =
    "<?php\nnamespace app\\modules\\shop\\models;\n\nclass Cart extends ActiveRecord\n{\n}\n";

const NG_CONTROLLER: &str = "angular.module('shop').controller('CartCtrl', function () {});\n";

/// Egy Yii-szerű PHP-projekt AngularJS-frontenddel, egy minifikált másolattal, két bemásolt külső csomaggal és
/// egy `Services/Event` domain-mappával.
fn legacy() -> Project {
    Project::new(&[
        ("composer.json", "{\"name\": \"acme/shop\"}\n"),
        (
            "modules/shop/controllers/CartController.php",
            CART_CONTROLLER,
        ),
        (
            "modules/shop/controllers/OrderController.php",
            ORDER_CONTROLLER,
        ),
        ("modules/shop/models/Cart.php", CART_MODEL),
        ("web/assets/frontend/js/controllers/cart.js", NG_CONTROLLER),
        (
            "web/assets.min/frontend/js/controllers/cart.js",
            NG_CONTROLLER,
        ),
        (
            "extensions/Facebook/composer.json",
            "{\"name\": \"facebook/graph-sdk\"}\n",
        ),
        (
            "extensions/Facebook/src/Builders/RequestBuilder.php",
            "<?php\nnamespace Facebook;\n\nclass RequestBuilder\n{\n}\n",
        ),
        (
            "extensions/Facebook/src/Factories/ClientFactory.php",
            "<?php\nnamespace Facebook;\n\nclass ClientFactory\n{\n}\n",
        ),
        (
            "sdk/composer.json",
            "{\"name\": \"vendorco/sdk\", \"type\": \"library\"}\n",
        ),
        (
            "sdk/Handlers/FooHandler.php",
            "<?php\nnamespace Vendorco;\n\nclass FooHandler\n{\n}\n",
        ),
        (
            "app/Services/Event/EventPlanner.php",
            "<?php\nnamespace App\\Services\\Event;\n\nclass EventPlanner\n{\n}\n",
        ),
    ])
}

#[test]
fn profile_separates_ecosystems_and_skips_vendor_code() {
    let project = legacy();
    let out = project.run(&["profile"]);
    let php = line_with(&out, "  controller (php) ");
    assert!(php.contains("modules/shop/controllers"), "{out}");
    assert!(php.contains("   2"), "{out}");
    let js = line_with(&out, "  controller (js) ");
    assert!(js.contains("web/assets/frontend/js/controllers"), "{out}");
    // a rétegek fájlszám szerint, azonos számnál név szerint
    let order: Vec<usize> = [
        "  controller (php) ",
        "  controller (js) ",
        "  model ",
        "  service ",
    ]
    .iter()
    .map(|l| out.find(l).unwrap_or_else(|| panic!("{l}: {out}")))
    .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{out}");
    // a domain-szó (Services/Event) nem réteg; a vendor-kódból nincs réteg és minta
    for missing in [
        "  event",
        "  builder",
        "  factory",
        "  handler",
        "Builder",
        "Factory",
    ] {
        assert!(!out.contains(missing), "{missing}: {out}");
    }
    assert!(
        out.contains("vendor: 3 files skipped (extensions/Facebook, sdk)\n"),
        "{out}"
    );
    assert!(
        out.contains("generated: 1 file skipped (web/assets.min)\n"),
        "{out}"
    );
    let hu = project.run_in(&["profile"], "hu");
    assert!(
        hu.contains("vendor: 3 fájl kihagyva (extensions/Facebook, sdk)\n"),
        "{hu}"
    );

    let with_vendor = project.run(&["profile", "--with-vendor"]);
    assert!(with_vendor.contains("Builder"), "{with_vendor}");
    assert!(!with_vendor.contains("vendor: "), "{with_vendor}");

    // a vendor-fájlok a lekérdezésekben megmaradnak
    let facebook = project.run(&["show", "extensions", "--full"]);
    assert!(facebook.contains("RequestBuilder.php"), "{facebook}");

    // --layer: minősítővel egy ökoszisztéma, nélküle mindkettő
    let only_js = project
        .run(&[
            "show",
            "modules",
            "web",
            "--layer",
            "controller:js",
            "--full",
        ])
        // Windowson a FILE sor `\`-t ír
        .replace('\\', "/");
    assert!(
        only_js.contains("frontend/js/controllers/cart.js"),
        "{only_js}"
    );
    assert!(!only_js.contains("CartController"), "{only_js}");
    let only_php = project.run(&[
        "show",
        "modules",
        "web",
        "--layer",
        "controller (php)",
        "--full",
    ]);
    assert!(only_php.contains("CartController"), "{only_php}");
    assert!(!only_php.contains("cart.js"), "{only_php}");
    let both = project.run(&["show", "modules", "web", "--layer", "controller", "--full"]);
    assert!(
        both.contains("CartController") && both.contains("cart.js"),
        "{both}"
    );
}

#[test]
fn by_layer_prints_a_header_per_layer_in_both_grep_views() {
    let project = laravel();
    let args = ["show", "app", "--grep", "UserRepositoryInterface::find"];
    let mut normal = args.to_vec();
    normal.push("--by-layer");
    let out = project.run(&normal);
    let service = out
        .find("[service]  1 call\nFILE  ")
        .unwrap_or_else(|| panic!("{out}"));
    let controller = out
        .find("[controller]  1 call\nFILE  ")
        .unwrap_or_else(|| panic!("{out}"));
    assert!(service < controller, "closest to the callee first: {out}");
    let plain = project.run(&args);
    assert!(!plain.contains("[service]"), "{plain}");

    let mut compact = normal.clone();
    compact.push("CALLGRAPH_OUT_LIMIT=6");
    let out = project.run(&compact);
    assert!(out.contains("1 line per call instead"), "{out}");
    let service = out
        .find("\n[service]  1 call\n")
        .unwrap_or_else(|| panic!("{out}"));
    let controller = out
        .find("\n[controller]  1 call\n")
        .unwrap_or_else(|| panic!("{out}"));
    assert!(service < controller, "{out}");
}

#[test]
fn unknown_layer_is_a_user_error_without_error_id() {
    let project = laravel();
    let skill = std::env::temp_dir().join(format!(
        "callgraph-profile-skill-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&skill).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .args(["wrap", "app", "--layer", "nope"])
        .current_dir(&project.0)
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_UPDATE_CHECK", "0")
        .env("CALLGRAPH_STATS", "0")
        .env("CALLGRAPH_SKILL_DIR", &skill)
        .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
        .output()
        .expect("callgraph runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("Unknown layer: nope"), "{stderr}");
    assert!(!stderr.contains("error-id"), "{stderr}");
    assert!(!skill.join("errors.jsonl").exists());
    let _ = fs::remove_dir_all(&skill);
}

const LANGUAGE_MODEL: &str = "<?php\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Language extends Model\n{\n    public static function default(): string\n    {\n        return 'en';\n    }\n}\n";

/// Egy `Event` domain-mappa (Actions / Services alatt), egy valódi Laravel-esemény, egy Mailable és egy PHP-enum.
fn events_domain() -> Project {
    Project::new(&[
        (
            "composer.json",
            "{\"require\": {\"laravel/framework\": \"^11\"}}\n",
        ),
        ("app/Models/Language.php", LANGUAGE_MODEL),
        (
            "app/Actions/Event/CreateEventAction.php",
            "<?php\nnamespace App\\Actions\\Event;\n\nuse App\\Models\\Language;\n\nclass CreateEventAction\n{\n    public function run(): string\n    {\n        return Language::default();\n    }\n}\n",
        ),
        (
            "app/Services/Event/EventPresenter.php",
            "<?php\nnamespace App\\Services\\Event;\n\nuse App\\Models\\Language;\n\nclass EventPresenter\n{\n    public function show(): string\n    {\n        return Language::default();\n    }\n}\n",
        ),
        (
            "app/Events/OrderShipped.php",
            "<?php\nnamespace App\\Events;\n\nuse App\\Models\\Language;\nuse Illuminate\\Foundation\\Events\\Dispatchable;\n\nclass OrderShipped\n{\n    use Dispatchable;\n\n    public function lang(): string\n    {\n        return Language::default();\n    }\n}\n",
        ),
        (
            "app/Mail/OrderMail.php",
            "<?php\nnamespace App\\Mail;\n\nuse Illuminate\\Mail\\Mailable;\n\nclass OrderMail extends Mailable\n{\n}\n",
        ),
        (
            "app/Enums/PaymentProvider.php",
            "<?php\nnamespace App\\Enums;\n\nenum PaymentProvider: string\n{\n    case Stripe = 'stripe';\n}\n",
        ),
        (
            "app/Support/ShippingProvider.php",
            "<?php\nnamespace App\\Support;\n\nenum ShippingProvider: string\n{\n    case Dhl = 'dhl';\n}\n",
        ),
    ])
}

#[test]
fn layer_switches_classify_like_the_profile() {
    let project = events_domain();
    let profile = project.run(&["profile", "--full"]);
    assert!(profile.contains("  event "), "{profile}");
    assert!(profile.contains("  mail "), "{profile}");
    assert!(
        profile.contains("confirmed 1: extends Mailable"),
        "{profile}"
    );
    // enum: a név (`*Provider`) nem szerep; az `Enums` mappa a saját (kiszolgáló) szerepe
    assert!(profile.contains("  enum "), "{profile}");
    assert!(!profile.contains("  provider "), "{profile}");
    let args = ["show", "app", "--grep", "Language::default", "--by-layer"];
    let out = project.run(&args);
    assert!(out.contains("[service]  2 calls\n"), "{out}");
    assert!(out.contains("[event]  1 call\n"), "{out}");
    // gráf nélkül a profil tartalék-szabálya: kód-jel nélkül nincs event
    let no_graph = project.command_env(&args, "en", &[("CALLGRAPH_NO_GRAPH", "1")]);
    let no_graph = String::from_utf8(no_graph.stdout).unwrap();
    assert!(no_graph.contains("[service]  2 calls\n"), "{no_graph}");
    assert!(!no_graph.contains("[event]"), "{no_graph}");
    // --layer event: csak a valódi esemény
    let events = project.run(&["show", "app", "--layer", "event", "--full"]);
    assert!(events.contains("OrderShipped"), "{events}");
    assert!(!events.contains("CreateEventAction"), "{events}");
    assert!(!events.contains("EventPresenter"), "{events}");
    // callers --by-layer: a JSON `layer` mezője is
    let callers = project.run(&["callers", "Language::default", "--by-layer"]);
    let layers: Vec<String> = callers
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v["layer"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(
        layers.iter().filter(|l| *l == "event").count(),
        1,
        "{callers}"
    );
    assert_eq!(
        layers.iter().filter(|l| *l == "service").count(),
        2,
        "{callers}"
    );
}
