//! A helyi használati mérés a valódi binárissal: `CALLGRAPH_STATS_DIR` mellett a `show` és a `locate` futásai a
//! napi összesítőbe kerülnek, és a `callgraph stats` kiírja őket; a változó nélkül (skill-telepítésen kívül)
//! semmi nem íródik. Hálózat nincs.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "callgraph-it-stats-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// Egy kis PHP-projekt.
fn project(root: &Path) {
    let files = [
        (
            "src/RefundService.php",
            "<?php\nnamespace App;\n\nclass RefundService\n{\n    public function processRefund(int $id): void\n    {\n    }\n}\n",
        ),
        (
            "src/Order.php",
            "<?php\nnamespace App;\n\nclass Order\n{\n    public function cancel(): void\n    {\n    }\n}\n",
        ),
    ];
    for (path, body) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
}

/// A bináris a projektben; `stats_dir`: a `CALLGRAPH_STATS_DIR` (vagy nincs), a skill-mappa egy ideiglenes mappa.
fn run(root: &Path, skill: &Path, stats_dir: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
    command
        .args(args)
        .current_dir(root)
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
        .env("CALLGRAPH_SKILL_DIR", skill)
        .env_remove("CALLGRAPH_STATS")
        .env_remove("CALLGRAPH_STATS_CLIENT")
        .env_remove("CALLGRAPH_STATS_DIR");
    if let Some(dir) = stats_dir {
        command.env("CALLGRAPH_STATS_DIR", dir);
    }
    command.output().unwrap()
}

/// A parancs sora a táblázatban: `[név, hívás, hiba, idő, kimenet, levágva]`.
fn row<'a>(report: &'a str, name: &str) -> Vec<&'a str> {
    report
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .find(|cells| cells.first() == Some(&name))
        .unwrap_or_else(|| panic!("no {name} row: {report}"))
}

#[test]
fn records_cli_runs_and_reports_them() {
    let root = temp_dir("project");
    let skill = temp_dir("skill");
    let stats = temp_dir("data");
    project(&root);
    let dir = Some(stats.as_path());

    let show = run(&root, &skill, dir, &["show", "src/Order.php"]);
    assert!(show.status.success(), "{show:?}");
    let missing = run(&root, &skill, dir, &["show", "src/Missing.php"]);
    assert!(!missing.status.success());
    let locate = run(&root, &skill, dir, &["locate", "processRefund", "refund"]);
    assert!(locate.status.success(), "{locate:?}");
    // a `stats` maga nem számít
    let first = run(&root, &skill, dir, &["stats"]);
    assert!(first.status.success(), "{first:?}");

    let files: Vec<_> = std::fs::read_dir(&stats).unwrap().collect();
    assert_eq!(files.len(), 1, "one month file");
    let text = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
    // út, fájl- és szimbólumnév nincs a napi fájlban
    for secret in ["/", "Order", "Missing", "processRefund", "refund\""] {
        assert!(!text.contains(secret), "{secret}: {text}");
    }

    let report = run(&root, &skill, dir, &["stats", "--days", "3"]);
    assert!(report.status.success(), "{report:?}");
    let report = String::from_utf8_lossy(&report.stdout);
    assert!(
        report.starts_with("callgraph stats · last 3 days"),
        "{report}"
    );
    assert!(report.contains(" · 3 calls: cli 3\n"), "{report}");
    assert_eq!(row(&report, "show")[1..3], ["2", "1"], "{report}");
    assert_eq!(row(&report, "locate")[1..3], ["1", "0"], "{report}");
    assert!(
        report.contains("nothing is sent anywhere") && report.contains("CALLGRAPH_STATS=0"),
        "{report}"
    );
    assert!(!report.contains("Recording is off"), "{report}");
    assert!(report.lines().count() <= 20, "{report}");

    // kikapcsolva: a futás nem ír, a már meglévő adat marad olvasható
    let before = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
    let off = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .args(["show", "src/Order.php"])
        .current_dir(&root)
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_SKILL_DIR", &skill)
        .env("CALLGRAPH_STATS_DIR", &stats)
        .env("CALLGRAPH_STATS", "0")
        .output()
        .unwrap();
    assert!(off.status.success());
    let after = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
    assert_eq!(before, after);

    for dir in [root, skill, stats] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn writes_nothing_outside_a_skill_install_without_the_env_var() {
    let root = temp_dir("dev-project");
    let skill = temp_dir("dev-skill");
    project(&root);
    let show = run(&root, &skill, None, &["show", "src/Order.php"]);
    assert!(show.status.success(), "{show:?}");
    let locate = run(&root, &skill, None, &["locate", "Order"]);
    assert!(locate.status.success(), "{locate:?}");
    assert!(!skill.join("stats").exists(), "stats written in a dev run");
    assert!(!root.join("stats").exists());

    // a riport ilyenkor barátságos egysoros + hogy hol lenne és hogy ki van kapcsolva
    let report = run(&root, &skill, None, &["stats"]);
    assert!(report.status.success(), "{report:?}");
    let report = String::from_utf8_lossy(&report.stdout);
    assert!(
        report.starts_with("callgraph stats: no usage recorded in the last 7 days."),
        "{report}"
    );
    assert!(report.contains("Recording is off here"), "{report}");
    assert!(!skill.join("stats").exists());

    let bad = run(&root, &skill, None, &["stats", "--days", "0"]);
    assert!(!bad.status.success());

    for dir in [root, skill] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
