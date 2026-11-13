//! Kódgenerátor (`callgraph gen` / `callgraph undo`) a binárison át, egy kis composer-projekten: új fájl, beszúrás,
//! csere, mentés, undo, szintaktikai hiba (semmi nem íródik), `--dry`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

const COMPOSER: &str = r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#;
const ORDER: &str =
    "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Models;\n\nclass Order\n{\n}\n";

/// Egyedi ideiglenes projekt; a végén törlődik.
struct TempProject {
    base: PathBuf,
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "callgraph-gen-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("project");
        fs::create_dir_all(root.join("app/Models")).unwrap();
        fs::create_dir_all(base.join("skill")).unwrap();
        fs::write(root.join("composer.json"), COMPOSER).unwrap();
        fs::write(root.join("app/Models/Order.php"), ORDER).unwrap();
        Self {
            root: root.canonicalize().unwrap(),
            base,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .arg("--project")
            .arg(&self.root)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_SKILL_DIR", self.base.join("skill"))
            .current_dir(&self.root)
            .output()
            .expect("callgraph runs")
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(rel)).unwrap()
    }

    fn exists(&self, rel: &str) -> bool {
        Path::new(&self.root).join(rel).exists()
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn gen_write_into_replace_and_undo_end_to_end() {
    let p = TempProject::new();

    // új fájl: névtér a composer psr-4-ből, import a projekt osztály-indexéből, strict_types a többségből
    let out = p.run(&[
        "gen",
        "c!:Pay > ctor(-Order $order) + +charge(int $amount):bool",
        "--write",
        "app/Services/Pay.php",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let line = stdout(&out);
    assert!(
        line.starts_with("wrote app/Services/Pay.php (") && line.ends_with(" · backup: none\n"),
        "{line}"
    );
    let text = p.read("app/Services/Pay.php");
    assert!(text.contains("declare(strict_types=1);"), "{text}");
    assert!(
        text.contains("namespace App\\Services;\n\nuse App\\Models\\Order;\n"),
        "{text}"
    );
    assert!(text.contains("final class Pay"), "{text}");
    assert!(text.contains("private Order $order,"), "{text}");
    let original = text;

    // létezőt nem ír felül
    let out = p.run(&["gen", "c:Pay", "--write", "app/Services/Pay.php"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).starts_with("gen: app/Services/Pay.php already exists"),
        "{}",
        stderr(&out)
    );
    assert_eq!(p.read("app/Services/Pay.php"), original);
    assert!(!p.exists(".callgraph/backups"));

    // beszúrás egy metódus után
    let out = p.run(&[
        "gen",
        "+refund(Order $o):void{ $this->last = $o; }",
        "--into",
        "app/Services/Pay.php::Pay",
        "--after",
        "charge",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let line = stdout(&out);
    assert!(
        line.starts_with("inserted 1 member(s) into app/Services/Pay.php::Pay after charge (L")
            && line.contains(" · backup: .callgraph/backups/app/Services/Pay.php."),
        "{line}"
    );
    let inserted = p.read("app/Services/Pay.php");
    assert!(
        inserted.contains("public function refund(Order $o): void"),
        "{inserted}"
    );

    // csere metódus szerint
    let out = p.run(&[
        "gen",
        "+refund(Order $o):bool{ return true; }",
        "--replace",
        "app/Services/Pay.php::refund",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).starts_with("replaced app/Services/Pay.php L"),
        "{}",
        stdout(&out)
    );
    let replaced = p.read("app/Services/Pay.php");
    assert!(
        replaced.contains("public function refund(Order $o): bool"),
        "{replaced}"
    );
    assert!(!replaced.contains("$this->last"), "{replaced}");

    // undo: visszaállítja a csere előtti állapotot, és maga is visszavonható
    let out = p.run(&["undo", "app/Services/Pay.php"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).starts_with("restored app/Services/Pay.php from .callgraph/backups/"),
        "{}",
        stdout(&out)
    );
    assert_eq!(p.read("app/Services/Pay.php"), inserted);
    let out = p.run(&["undo"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(p.read("app/Services/Pay.php"), replaced);
}

#[test]
fn a_syntax_error_writes_nothing_and_exits_with_2() {
    let p = TempProject::new();
    let out = p.run(&[
        "gen",
        "c:Bad > +f():int{ return ( ; }",
        "--write",
        "app/Bad.php",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).starts_with("gen: syntax error at L"),
        "{}",
        stderr(&out)
    );
    assert!(!p.exists("app/Bad.php"));

    let before = p.read("app/Models/Order.php");
    let out = p.run(&[
        "gen",
        "+f():int{ return ( ; }",
        "--into",
        "app/Models/Order.php",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(p.read("app/Models/Order.php"), before);
    assert!(!p.exists(".callgraph/backups"));
}

#[test]
fn dry_prints_the_code_and_writes_nothing() {
    let p = TempProject::new();
    let out = p.run(&[
        "gen",
        "c:Dry > +items():iter<int>{ yield 1; } + +x():void",
        "--dry",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let code = stdout(&out);
    assert!(code.contains("class Dry\n{"), "{code}");
    assert!(code.contains("yield 1;"), "{code}");
    assert!(code.contains("@return iterable<int>"), "{code}");
    assert!(!p.exists("app/Dry.php"));

    // csupasz tagok is: kód, fájl nélkül
    let out = p.run(&["gen", "-sign(array $payload):string"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("private function sign(array $payload): string"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn unknown_names_are_listed_not_guessed() {
    let p = TempProject::new();
    let out = p.run(&["gen", "c:A > +f(Mystery $m):void", "--dry"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).ends_with("unresolved: Mystery\n"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn the_help_documents_the_generator() {
    let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .arg("--help")
        .env("CALLGRAPH_LANG", "en")
        .output()
        .expect("callgraph runs");
    let help = stdout(&out);
    assert!(help.contains("Code generator"), "{help}");
    assert!(help.contains("callgraph undo [<file>]"), "{help}");
}
