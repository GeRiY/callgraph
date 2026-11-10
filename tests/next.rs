//! A javasolt következő lépés a valódi binárissal: egy kis PHP-projekten a `locate` `CALLGRAPH_NEXT=1` mellett egy
//! `↪ rN …` sorral végződik, és a `next rN 1` az 1. fájl outline-ját írja ki; a változó nélkül nincs sor; az
//! ismeretlen azonosító használati hiba.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "callgraph-it-next-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::write(
        dir.join("app/OrderService.php"),
        "<?php\nnamespace App;\n\nclass OrderService\n{\n    public function refund(int $id): void\n    {\n        $this->log($id);\n    }\n\n    private function log(int $id): void\n    {\n    }\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app/RefundJob.php"),
        "<?php\nnamespace App;\n\nclass RefundJob\n{\n    public function handle(OrderService $s): void\n    {\n        $s->refund(1);\n    }\n}\n",
    )
    .unwrap();
    dir.canonicalize().unwrap()
}

fn run(root: &Path, next: bool, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
    command
        .args(args)
        .current_dir(root)
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
        .env("CALLGRAPH_STATS", "0")
        .env("CALLGRAPH_UPDATE_CHECK", "0")
        .env_remove("CALLGRAPH_NEXT");
    if next {
        command.env("CALLGRAPH_NEXT", "1");
    }
    command.output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn locate_carries_a_suggestion_line_and_next_runs_it() {
    let root = temp_dir();
    let plain = run(&root, false, &["locate", "OrderService", "refund"]);
    assert!(plain.status.success(), "{plain:?}");
    assert!(!stdout(&plain).contains('↪'), "{}", stdout(&plain));

    let located = run(&root, true, &["locate", "OrderService", "refund"]);
    assert!(located.status.success(), "{located:?}");
    let text = stdout(&located);
    let last = text.lines().last().unwrap();
    assert!(
        last.starts_with("↪ r1 possible next: [1] outline app/OrderService.php"),
        "{text}"
    );
    assert!(root.join(".callgraph/results/results.jsonl").is_file());

    let next = run(&root, true, &["next", "r1", "1"]);
    assert!(next.status.success(), "{next:?}");
    let outline = stdout(&next);
    assert!(outline.contains("OrderService.php"), "{outline}");
    assert!(outline.contains("refund()  L"), "{outline}");
    // a `next` kimenete is azonos azzal, mintha a felhasználó futtatta volna
    let direct = run(&root, false, &["wrap", "app/OrderService.php"]);
    assert!(outline.starts_with(&stdout(&direct)), "{outline}\n---\n{}", stdout(&direct));

    let unknown = run(&root, true, &["next", "r99", "1"]);
    assert_eq!(unknown.status.code(), Some(2), "{unknown:?}");
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("r99"));
    let bad = run(&root, true, &["next", "r1", "7"]);
    assert_eq!(bad.status.code(), Some(2), "{bad:?}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn machine_formats_never_get_a_line() {
    let root = temp_dir();
    for format in ["json", "mermaid", "dot"] {
        let args = ["wrap", "app/OrderService.php", "--format", format];
        let out = run(&root, true, &args);
        assert!(out.status.success(), "{out:?}");
        assert!(!stdout(&out).contains('↪'), "{format}: {}", stdout(&out));
    }
    let _ = std::fs::remove_dir_all(root);
}
