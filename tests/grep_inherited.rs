//! `--grep Osztály::metódus`: a leszármazott osztályok örökölt `$this->metódus()` hívásai is találatok
//! (a saját példányon át hívott metódus a hívó típusán), a metódust felüldefiniáló leszármazotté nem.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

impl Project {
    /// Egy szülő-controller a metódussal, két leszármazott (másik névtérben is) örökölt `$this->` hívással,
    /// és egy harmadik, amely felüldefiniálja (az ő hívása nem a keresett metódusé).
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-inherited-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        let files = [
            (
                "controllers/FrontendController.php",
                "<?php\nnamespace app\\controllers;\n\nclass FrontendController\n{\n    protected function redirectToNewDashboard($id)\n    {\n        return $id;\n    }\n}\n",
            ),
            (
                "controllers/EventeditController.php",
                "<?php\nnamespace app\\controllers;\n\nclass EventeditController extends FrontendController\n{\n    public function actionSave($id)\n    {\n        return $this->redirectToNewDashboard($id);\n    }\n}\n",
            ),
            (
                "controllers/front/DashboardController.php",
                "<?php\nnamespace app\\controllers\\front;\n\nuse app\\controllers\\FrontendController;\n\nclass DashboardController extends FrontendController\n{\n    public function actionIndex()\n    {\n        return $this->redirectToNewDashboard(5);\n    }\n}\n",
            ),
            (
                "controllers/OwnController.php",
                "<?php\nnamespace app\\controllers;\n\nclass OwnController extends FrontendController\n{\n    protected function redirectToNewDashboard($id)\n    {\n        return 0;\n    }\n\n    public function actionGo()\n    {\n        return $this->redirectToNewDashboard(1);\n    }\n}\n",
            ),
        ];
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self(root.canonicalize().unwrap())
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .output()
            .expect("callgraph runs");
        assert!(
            out.status.success(),
            "{}",
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

#[test]
fn qualified_search_finds_inherited_this_calls_in_subclasses() {
    let project = Project::new();
    let out = project.run(&[
        "show",
        "controllers",
        "--grep",
        "FrontendController::redirectToNewDashboard",
    ]);
    assert!(
        out.contains("L8 via EventeditController │ return $this->redirectToNewDashboard($id);"),
        "{out}"
    );
    assert!(
        out.contains("L10 via DashboardController │ return $this->redirectToNewDashboard(5);"),
        "{out}"
    );
    // a felüldefiniáló leszármazott saját metódusát hívja, nem a keresettet
    assert!(!out.contains("OwnController"), "{out}");
}

#[test]
fn qualified_search_on_the_subclass_finds_its_own_this_call() {
    let project = Project::new();
    let out = project.run(&[
        "show",
        "controllers",
        "--grep",
        "OwnController::redirectToNewDashboard",
    ]);
    assert!(
        out.contains("L13 │ return $this->redirectToNewDashboard(1);"),
        "{out}"
    );
    assert!(!out.contains("EventeditController"), "{out}");
    assert!(!out.contains("DashboardController"), "{out}");
}

#[test]
fn bare_method_name_still_finds_every_caller() {
    let project = Project::new();
    let out = project.run(&["show", "controllers", "--grep", "redirectToNewDashboard"]);
    for class in [
        "EventeditController",
        "DashboardController",
        "OwnController",
    ] {
        assert!(out.contains(class), "{class}: {out}");
    }
}

#[test]
fn depth_search_finds_inherited_this_calls_in_subclasses() {
    let project = Project::new();
    let out = project.run(&[
        "show",
        "controllers",
        "--grep",
        "FrontendController::redirectToNewDashboard",
        "--depth",
        "2",
    ]);
    assert!(out.contains("EventeditController::actionSave()"), "{out}");
    assert!(out.contains("DashboardController::actionIndex()"), "{out}");
    assert!(out.contains("L8 via EventeditController │"), "{out}");
    assert!(out.contains("L10 via DashboardController │"), "{out}");
    assert!(!out.contains("OwnController"), "{out}");
}

#[test]
fn callers_jsonl_lists_inherited_this_calls() {
    let project = Project::new();
    let out = project.run(&["callers", "FrontendController::redirectToNewDashboard"]);
    let rows: Vec<serde_json::Value> = out
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut callers: Vec<(String, String)> = rows
        .iter()
        .map(|row| {
            (
                row["caller"].as_str().unwrap_or_default().to_owned(),
                row["via"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    callers.sort();
    assert_eq!(
        callers,
        vec![
            ("actionIndex".to_owned(), "DashboardController".to_owned()),
            ("actionSave".to_owned(), "EventeditController".to_owned()),
        ],
        "{out}"
    );
}
