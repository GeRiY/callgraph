//! A Laravel-konténerből kért példány (`app(Foo::class)`, `resolve(Foo::class)`, láncolt `->m()`) hívásai a valódi
//! binárissal: a `--grep … --depth 2` `--guess` nélkül is megtalálja a hívókat, az outline a feloldott célt mutatja.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-php-container-it-{}-{id}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// `show --project <gyökér> …` angol kimenettel.
    fn show(&self, args: &[&str]) -> String {
        let root = self.0.to_string_lossy().into_owned();
        let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(["show", "--project", root.as_str()])
            .args(args)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
            .env_remove("CALLGRAPH_GITIGNORE")
            .output()
            .expect("callgraph runs");
        assert!(
            out.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
        String::from_utf8(out.stdout).unwrap().replace('\\', "/")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Egy kis Laravel-szerű projekt: a szolgáltatást a konténerből kérik (változón át és láncolva).
fn laravel() -> Project {
    let project = Project::new();
    project.write(
        "app/Services/UserService.php",
        "<?php\nnamespace App\\Services;\n\nclass UserService\n{\n    public function createUser(string $name): string\n    {\n        return $name;\n    }\n}\n",
    );
    project.write(
        "tests/Integration/UserServiceTest.php",
        "<?php\nnamespace Tests\\Integration;\n\nuse App\\Services\\UserService;\n\nclass UserServiceTest\n{\n    public function testCreatesAUser(): void\n    {\n        $service = app(UserService::class);\n        $service->createUser('a');\n    }\n}\n",
    );
    project.write(
        "app/Http/Controllers/UserController.php",
        "<?php\nnamespace App\\Http\\Controllers;\n\nuse App\\Services\\UserService;\n\nclass UserController\n{\n    public function store(): string\n    {\n        return resolve(UserService::class)->createUser('b');\n    }\n}\n",
    );
    project.write(
        "app/Console/SeedUsers.php",
        "<?php\nnamespace App\\Console;\n\nuse App\\Http\\Controllers\\UserController;\n\nclass SeedUsers\n{\n    public function handle(): void\n    {\n        app(UserController::class)->store();\n    }\n}\n",
    );
    project
}

#[test]
fn container_resolved_callers_are_found_without_guess() {
    let project = laravel();
    let out = project.show(&[".", "--grep", "UserService::createUser", "--depth", "2"]);
    let (first, second) = out
        .split_once("== level 2")
        .unwrap_or_else(|| panic!("{out}"));
    assert!(
        first.contains("UserServiceTest::testCreatesAUser()"),
        "{out}"
    );
    assert!(first.contains("UserController::store()"), "{out}");
    assert!(second.contains("SeedUsers::handle()"), "{out}");
}

#[test]
fn the_outline_shows_the_resolved_target() {
    let project = laravel();
    let outline = project.show(&[
        "tests/Integration/UserServiceTest.php",
        "--format",
        "outline",
    ]);
    assert!(
        outline.contains("App/Services/UserService::createUser"),
        "{outline}"
    );
    let outline = project.show(&["app/Console/SeedUsers.php", "--format", "outline"]);
    assert!(
        outline.contains("App/Http/Controllers/UserController::store"),
        "{outline}"
    );
}
