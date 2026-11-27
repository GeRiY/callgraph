//! `callgraph impact`: the changed symbols of a diff with their callers, framework links and tests. The diff comes from
//! stdin (`--diff -`), so the tests need no git; the git runner has its own tests at the end (skipped without git).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// A temporary project; the cache lives inside it.
struct Project(PathBuf);

fn unique_dir(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "callgraph-impact-{tag}-{id}-{}-{n}",
        std::process::id()
    ))
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        let root = unique_dir("p");
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self(root.canonicalize().unwrap())
    }

    /// A copy of the Laravel + Inertia fixture of the web plan, plus extra files.
    fn laravel(extra: &[(&str, &str)]) -> Self {
        let root = unique_dir("laravel");
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/web/laravel_app"),
            &root,
        );
        for (path, text) in extra {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self(root.canonicalize().unwrap())
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
            .env_remove("CALLGRAPH_OUT_LIMIT");
        command
    }

    /// `impact <args> --diff -` with the diff on stdin; the raw output.
    fn impact_raw(&self, args: &[&str], diff: &str) -> Output {
        let mut all = vec!["impact"];
        all.extend_from_slice(args);
        all.extend_from_slice(&["--diff", "-"]);
        let mut child = self
            .command(&all)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("callgraph runs");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(diff.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn impact(&self, args: &[&str], diff: &str) -> String {
        let out = self.impact_raw(args, diff);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn root(&self) -> String {
        let root = self.0.to_string_lossy().into_owned();
        match root.strip_prefix(r"\\?\") {
            Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_owned(),
            _ => root,
        }
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A one-hunk diff replacing line `line` of `file`.
fn one_line(file: &str, line: u32) -> String {
    format!(
        "diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n@@ -{line} +{line} @@\n-old\n+new\n"
    )
}

const ORGANIZATION: &str = "<?php\nnamespace App\\Models;\n\nclass Organization\n{\n    public function contacts(int $id): array\n    {\n        return [];\n    }\n\n    public function label(): string\n    {\n        return 'org';\n    }\n}\n";

const ORGANIZATIONS_CONTROLLER: &str = "<?php\nnamespace App\\Http\\Controllers;\n\nuse App\\Models\\Organization;\n\nclass OrganizationsController\n{\n    private Organization $org;\n\n    public function edit(int $id): array\n    {\n        return $this->org->contacts($id);\n    }\n}\n";

const CONTACTS_CONTROLLER: &str = "<?php\nnamespace App\\Http\\Api;\n\nuse App\\Http\\Controllers\\OrganizationsController;\n\nclass ContactsController\n{\n    private OrganizationsController $orgs;\n\n    public function index(int $id): array\n    {\n        return $this->orgs->edit($id);\n    }\n}\n";

const ORGANIZATIONS_TEST: &str = "<?php\nnamespace Tests\\Feature;\n\nuse App\\Models\\Organization;\n\nclass OrganizationsTest\n{\n    private Organization $org;\n\n    public function test_contacts(): void\n    {\n        $this->org->contacts(1);\n    }\n}\n";

fn php_project() -> Project {
    Project::new(&[
        ("app/Models/Organization.php", ORGANIZATION),
        (
            "app/Http/Controllers/OrganizationsController.php",
            ORGANIZATIONS_CONTROLLER,
        ),
        ("app/Http/Api/ContactsController.php", CONTACTS_CONTROLLER),
        ("tests/Feature/OrganizationsTest.php", ORGANIZATIONS_TEST),
        ("README.md", "# readme\n"),
    ])
}

#[test]
fn a_changed_method_lists_its_callers_tests_and_the_other_files() {
    let project = php_project();
    let diff = format!(
        "{}{}",
        one_line("app/Models/Organization.php", 8),
        one_line("README.md", 1)
    );
    let out = project.impact(&[], &diff);
    assert_eq!(
        out.lines().next(),
        Some(format!("root: {}", project.root()).as_str()),
        "{out}"
    );
    assert!(
        out.lines()
            .nth(1)
            .unwrap()
            .starts_with("impact: 1 changed symbol in 1 file · 1 caller · tests: 1 file"),
        "{out}"
    );
    assert!(
        out.contains(
            "app/Models/Organization.php\n  Organization::contacts()  L6-9  (changed L8)\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("    callers: OrganizationsController::edit L12\n"),
        "{out}"
    );
    assert!(
        out.contains("    tests: tests/Feature/OrganizationsTest.php::test_contacts\n"),
        "{out}"
    );
    assert!(
        out.contains("also changed: 1 other files (README.md)"),
        "{out}"
    );
    // the L1 style: no blank lines, no tips, the root once
    assert!(!out.contains("\n\n"), "{out}");
    assert_eq!(out.matches(project.root().as_str()).count(), 1, "{out}");
}

#[test]
fn depth_two_adds_the_callers_of_the_callers() {
    let project = php_project();
    let diff = one_line("app/Models/Organization.php", 8);
    let shallow = project.impact(&[], &diff);
    assert!(!shallow.contains("ContactsController"), "{shallow}");
    let deep = project.impact(&["--depth", "2"], &diff);
    assert!(
        deep.contains("    depth 2: ContactsController::index L12\n"),
        "{deep}"
    );
    assert!(deep.contains("· 2 callers ·"), "{deep}");
}

#[test]
fn a_changed_line_outside_any_method_maps_to_the_class() {
    let project = php_project();
    // L8 of the controller is the property
    let out = project.impact(
        &[],
        &one_line("app/Http/Controllers/OrganizationsController.php", 8),
    );
    assert!(
        out.contains("\n  class OrganizationsController  L6-14  (changed L8)\n"),
        "{out}"
    );
}

#[test]
fn compact_is_one_line_per_symbol_and_json_is_structured() {
    let project = php_project();
    let diff = format!(
        "{}{}",
        one_line("app/Models/Organization.php", 8),
        one_line("app/Models/Organization.php", 13)
    );
    let compact = project.impact(&["--compact"], &diff);
    let lines: Vec<&str> = compact
        .lines()
        .filter(|line| line.starts_with("app/Models/Organization.php  "))
        .collect();
    assert_eq!(lines.len(), 2, "{compact}");
    assert!(
        lines[0].contains("Organization::contacts()  L6-9"),
        "{compact}"
    );
    assert!(lines[0].contains(" 1 · "), "{compact}");
    assert!(
        lines[1].contains("Organization::label()  L11-14"),
        "{compact}"
    );

    let json = project.impact(&["--format", "json"], &diff);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["summary"]["symbols"], 2, "{json}");
    assert_eq!(value["summary"]["testFiles"], 1, "{json}");
    let symbols = &value["files"][0]["symbols"];
    assert_eq!(symbols[0]["name"], "contacts");
    assert_eq!(symbols[0]["container"], "Organization");
    assert_eq!(symbols[0]["callers"][0]["caller"], "edit");
    assert_eq!(symbols[0]["callers"][0]["line"], 12);
    assert_eq!(
        symbols[0]["tests"][0]["file"],
        "tests/Feature/OrganizationsTest.php"
    );
    assert_eq!(symbols[1]["callers"].as_array().unwrap().len(), 0);
}

#[test]
fn no_changes_and_unknown_files_are_one_line() {
    let project = php_project();
    let empty = project.impact(&[], "");
    assert!(empty.contains("impact: no changes\n"), "{empty}");
    // a code file the index does not know (ignored or outside the project): listed, not an error
    let unknown = project.impact(&[], &one_line("vendor/Missing.php", 3));
    assert!(
        unknown.contains("also changed: 1 other files (vendor/Missing.php)"),
        "{unknown}"
    );
}

#[test]
fn new_deleted_renamed_and_binary_files_are_handled() {
    let project = php_project();
    let plus = "+x\n".repeat(15);
    let diff = format!(
        "diff --git a/app/Models/Organization.php b/app/Models/Organization.php\nnew file mode 100644\n--- /dev/null\n+++ b/app/Models/Organization.php\n@@ -0,0 +1,15 @@\n{plus}diff --git a/app/Gone.php b/app/Gone.php\ndeleted file mode 100644\n--- a/app/Gone.php\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-x\n-y\ndiff --git a/logo.png b/logo.png\nBinary files a/logo.png and b/logo.png differ\n"
    );
    let out = project.impact(&[], &diff);
    assert!(
        out.contains("app/Models/Organization.php  (new file)\n"),
        "{out}"
    );
    assert!(out.contains("deleted: app/Gone.php\n"), "{out}");
    assert!(
        out.contains("also changed: 1 other files (logo.png)"),
        "{out}"
    );
    // a rename keeps the old name
    let renamed = "diff --git a/app/Old.php b/app/Models/Organization.php\nsimilarity index 80%\nrename from app/Old.php\nrename to app/Models/Organization.php\n@@ -8 +8 @@\n-a\n+b\n";
    let out = project.impact(&[], renamed);
    assert!(out.contains("(renamed from app/Old.php)"), "{out}");
    assert!(out.contains("Organization::contacts()"), "{out}");
}

#[test]
fn a_pure_deletion_names_the_symbol_around_it() {
    let project = Project::new(&[(
        "app/cart.py",
        "class Cart:\n    def total(self):\n        return self.subtotal() + 1\n\n    def subtotal(self):\n        base = 10\n        return base\n",
    )]);
    // two lines were removed after new line 6
    let diff = "diff --git a/app/cart.py b/app/cart.py\n--- a/app/cart.py\n+++ b/app/cart.py\n@@ -7,2 +6,0 @@ def subtotal\n-    x = 1\n-    y = 2\n";
    let out = project.impact(&[], diff);
    assert!(
        out.contains("Cart::subtotal()  L5-7  (lines removed near L6)"),
        "{out}"
    );
}

#[test]
fn python_self_calls_are_the_callers_and_name_only_matches_are_marked() {
    let project = Project::new(&[
        (
            "app/cart.py",
            "class Cart:\n    def total(self):\n        return self.subtotal() + 1\n\n    def subtotal(self):\n        return 10\n",
        ),
        (
            "app/main.py",
            "from app.cart import Cart\n\n\ndef checkout(cart):\n    return cart.total()\n",
        ),
    ]);
    let out = project.impact(&[], &one_line("app/cart.py", 3));
    assert!(
        out.contains("  Cart::total()  L2-3  (changed L3)\n"),
        "{out}"
    );
    // the receiver of `cart.total()` is unknown: a name-only match (marked with `?`)
    assert!(out.contains("checkout L5"), "{out}");
    let sub = project.impact(&[], &one_line("app/cart.py", 6));
    assert!(sub.contains("    callers: Cart::total L3\n"), "{sub}");
}

#[test]
fn a_controller_change_shows_route_page_and_validation() {
    let project = Project::laravel(&[(
        "tests/Feature/ShelfTest.php",
        "<?php\nnamespace Tests\\Feature;\n\nuse App\\Http\\Requests\\StoreShelfRequest;\n\nclass ShelfTest\n{\n    private StoreShelfRequest $request;\n\n    public function test_invalidate_clears_the_draft(): void\n    {\n        $this->request->invalidate();\n    }\n}\n",
    )]);
    let diff = format!(
        "{}{}{}",
        one_line("app/Http/Controllers/ShelfController.php", 23),
        one_line("app/Http/Controllers/ShelfController.php", 30),
        one_line("app/Http/Requests/StoreShelfRequest.php", 24)
    );
    let out = project.impact(&[], &diff);
    assert!(
        out.contains("  ShelfController::index()  L13-26  (changed L23)\n    route GET /shelves (shelves.index)\n    page Shelves/Browse\n"),
        "{out}"
    );
    assert!(
        out.contains("  ShelfController::store()  L28-35  (changed L30)\n    route POST /shelves (shelves.store)\n    rules StoreShelfRequest: stocked_at, title*\n    callers: none in code (route entry)\n"),
        "{out}"
    );
    assert!(out.contains("StoreShelfRequest::invalidate()"), "{out}");
    // the controller calls it (name-only or typed), and the test does too
    assert!(out.contains("ShelfController::store L32"), "{out}");
    assert!(
        out.contains("tests/Feature/ShelfTest.php::test_invalidate_clears_the_draft"),
        "{out}"
    );
    assert!(
        out.contains(" · 2 routes · 1 page · tests: 1 file"),
        "{out}"
    );
}

#[test]
fn a_changed_page_file_shows_where_it_is_rendered_and_its_route() {
    let project = Project::laravel(&[]);
    let out = project.impact(&[], &one_line("resources/js/pages/Shelves/Browse.vue", 17));
    assert!(out.contains("  page Shelves/Browse"), "{out}");
    assert!(out.contains("rendered by ShelfController::index"), "{out}");
    assert!(
        out.contains("  route GET /shelves (shelves.index)\n"),
        "{out}"
    );
}

#[test]
fn long_output_falls_back_to_one_line_per_symbol_and_is_cut_at_the_cap() {
    let mut methods = String::new();
    for n in 0..12 {
        methods.push_str(&format!(
            "    public function m{n}(): int\n    {{\n        return {n};\n    }}\n\n"
        ));
    }
    let source = format!("<?php\nnamespace App;\n\nclass Many\n{{\n{methods}}}\n");
    let project = Project::new(&[("app/Many.php", source.as_str())]);
    // every `return` line (the 3rd of each 5-line method, starting at L6) changed
    let mut diff = String::from("--- a/app/Many.php\n+++ b/app/Many.php\n");
    for n in 0..12 {
        let line = 8 + n * 5;
        diff.push_str(&format!("@@ -{line} +{line} @@\n-old\n+new\n"));
    }
    let mut command = project.command(&["impact", "--diff", "-"]);
    command
        .env("CALLGRAPH_OUT_LIMIT", "10")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(diff.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("⚠ impact output would be"), "{text}");
    assert!(text.lines().count() <= 10, "{text}");
    assert!(text.contains("more lines"), "{text}");
    // --full lifts the cap
    let mut command = project.command(&["impact", "--full", "--diff", "-"]);
    command
        .env("CALLGRAPH_OUT_LIMIT", "10")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(diff.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains("⚠ impact output would be"), "{text}");
    assert!(text.contains("Many::m11()"), "{text}");
}

#[test]
fn a_bad_depth_is_a_usage_error() {
    let project = php_project();
    let out = project
        .command(&["impact", "--depth", "9"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("callgraph impact"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---------------------------------------------------------------------------------------------------------------
// the git runner

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A committed repository with the PHP project.
fn repo() -> Project {
    let project = php_project();
    git(&project.0, &["init", "-q"]);
    git(&project.0, &["config", "user.email", "t@example.com"]);
    git(&project.0, &["config", "user.name", "t"]);
    // the cache of callgraph is never committed
    fs::write(project.0.join(".gitignore"), ".callgraph/\n").unwrap();
    git(&project.0, &["add", "-A"]);
    git(&project.0, &["commit", "-q", "-m", "init"]);
    project
}

#[test]
fn git_working_tree_staged_range_and_untracked() {
    if !git_available() {
        return;
    }
    let project = repo();
    let model = project.0.join("app/Models/Organization.php");
    fs::write(&model, ORGANIZATION.replace("return [];", "return [1];")).unwrap();
    // an untracked new file counts as a new file
    fs::write(
        project.0.join("app/Models/Contact.php"),
        "<?php\nnamespace App\\Models;\n\nclass Contact\n{\n    public function name(): string\n    {\n        return 'x';\n    }\n}\n",
    )
    .unwrap();

    let run = |args: &[&str]| -> Output { project.command(args).output().unwrap() };
    let out = run(&["impact"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("Organization::contacts()  L6-9  (changed L8)"),
        "{text}"
    );
    assert!(
        text.contains("callers: OrganizationsController::edit L12"),
        "{text}"
    );
    assert!(
        text.contains("app/Models/Contact.php  (new file)"),
        "{text}"
    );
    assert!(text.contains("Contact::name()"), "{text}");

    // nothing is staged yet
    let staged = String::from_utf8(run(&["impact", "--staged"]).stdout).unwrap();
    assert!(staged.contains("impact: no changes"), "{staged}");
    git(&project.0, &["add", "app/Models/Organization.php"]);
    let staged = String::from_utf8(run(&["impact", "--staged"]).stdout).unwrap();
    assert!(staged.contains("Organization::contacts()"), "{staged}");
    assert!(!staged.contains("Contact::name()"), "{staged}");

    // a committed change is found through a range
    git(&project.0, &["commit", "-q", "-m", "change contacts"]);
    let none = String::from_utf8(run(&["impact"]).stdout).unwrap();
    assert!(!none.contains("Organization::contacts()"), "{none}");
    let range = String::from_utf8(run(&["impact", "--diff", "HEAD~1"]).stdout).unwrap();
    assert!(range.contains("Organization::contacts()"), "{range}");
    let dots = String::from_utf8(run(&["impact", "--diff", "HEAD~1...HEAD"]).stdout).unwrap();
    assert!(dots.contains("Organization::contacts()"), "{dots}");
}

#[test]
fn outside_a_repository_or_with_a_bad_range_it_is_one_error_line() {
    if !git_available() {
        return;
    }
    // a directory outside any repository
    let project = php_project();
    let out = project.command(&["impact"]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(err.trim_end().lines().count(), 1, "{err}");
    assert!(err.starts_with("impact: git diff failed"), "{err}");
    // an option-looking range never reaches git
    let out = project
        .command(&["impact", "--diff=--output=x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!project.0.join("x").exists());
}
