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
        Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", lang)
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
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
    assert!(
        out.starts_with("profile: laravel (layered MVC) · confidence: high\nlayers (files):\n"),
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
    assert!(layered.contains("  [service]"), "{layered}");
    let mut a: Vec<String> = plain.lines().map(str::to_owned).collect();
    let mut b: Vec<String> = untagged(&layered).lines().map(str::to_owned).collect();
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

#[test]
fn profile_without_code_files_says_so() {
    let project = Project::new(&[("docs/readme.md", "# x\n")]);
    let out = project.run(&["profile"]);
    assert!(out.starts_with("profile: no code files"), "{out}");
}
