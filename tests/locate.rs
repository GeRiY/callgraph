//! `callgraph locate`: kiindulópont-kereső egy kis PHP + Vue projekten — a legjobb fájl elöl, indoklással; `--all`;
//! a találat nélküli és a túl általános kifejezések; a `.callgraphignore`-ral kizárt, a generált és a bináris fájl
//! sosem jelenik meg; a projekt-gráffal és nélküle (`CALLGRAPH_NO_GRAPH=1`) azonos a kimenet.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

impl Project {
    fn empty() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-locate-{id}-{}-{}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }

    /// A visszatérítéses mintaprojekt.
    fn refunds() -> Self {
        let project = Self::empty();
        project.write(
            "app/Http/Controllers/RefundController.php",
            "<?php\nnamespace App\\Http\\Controllers;\n\nuse App\\Services\\RefundService;\n\nclass RefundController\n{\n    public function __construct(private RefundService $service)\n    {\n    }\n\n    public function store(int $id)\n    {\n        return $this->service->processRefund($id);\n    }\n}\n",
        );
        project.write(
            "app/Services/RefundService.php",
            "<?php\nnamespace App\\Services;\n\nclass RefundService\n{\n    public function processRefund(int $id): string\n    {\n        return 'done';\n    }\n}\n",
        );
        project.write(
            "app/Jobs/RefundJob.php",
            "<?php\nnamespace App\\Jobs;\n\nuse App\\Services\\RefundService;\n\nclass RefundJob\n{\n    public function handle(): void\n    {\n        (new RefundService())->processRefund(1);\n    }\n}\n",
        );
        project.write(
            "app/Models/Order.php",
            "<?php\nnamespace App\\Models;\n\nclass Order\n{\n    public function paid()\n    {\n        return $this->where('payment_status', 'paid');\n    }\n}\n",
        );
        project.write(
            "database/migrations/2024_01_10_add_refunds_table.php",
            "<?php\n\nfunction up()\n{\n    return 'refund_reason';\n}\n",
        );
        project.write(
            "resources/js/pages/checkout.vue",
            "<template>\n  <button @click=\"submit\">Pay</button>\n</template>\n\n<script setup>\nimport api from '../api';\n\nfunction submit() {\n  api.refund(1);\n}\n</script>\n",
        );
        // kizárt / generált / bináris: a nevük illeszkedne, de nem jelenhetnek meg
        project.write(".callgraphignore", "md\n");
        project.write(
            "docs/refund.md",
            "# Refund\n\nprocessRefund RefundController\n",
        );
        project.write(
            "dist/refund.min.js",
            "function processRefund(){return 'refund'}\n",
        );
        let binary = project.0.join("assets/refund.js");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(binary, b"refund processRefund\0\x01\x02\xff\xfe binary").unwrap();
        project
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn command(&self, args: &[&str], graph: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command
            .arg("locate")
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH");
        if !graph {
            command.env("CALLGRAPH_NO_GRAPH", "1");
        }
        command.output().expect("callgraph runs")
    }

    fn run(&self, args: &[&str], graph: bool) -> String {
        let out = self.command(args, graph);
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

const TERMS: [&str; 5] = [
    "RefundController",
    "processRefund",
    "refund",
    "payment_status",
    "checkout.vue",
];

/// A rangsor fájl-sorai (`1. út  ★★★`): az utak a gyökérhez relatívan (egyetlen fájlnál a kimenet abszolút utat
/// ír), a `/` elválasztóval.
fn ranked(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| line.split_once(". "))
        .filter(|(n, _)| n.parse::<usize>().is_ok())
        .map(|(_, rest)| rest.split("  ").next().unwrap_or(rest))
        .map(|path| {
            let path = path.replace('\\', "/");
            let start = path
                .split_once("/app/")
                .or_else(|| path.split_once("/resources/"))
                .filter(|(prefix, _)| std::path::Path::new(prefix).is_absolute())
                .map(|(prefix, _)| prefix.len() + 1);
            match start {
                Some(at) => path[at..].to_owned(),
                None => path,
            }
        })
        .collect()
}

#[test]
fn the_best_starting_file_comes_first_with_its_reasons() {
    let project = Project::refunds();
    let out = project.run(&TERMS, true);
    assert!(
        out.contains("locate: 5 terms · ") && out.contains(" files · start here ↓"),
        "{out}"
    );
    let files = ranked(&out);
    assert_eq!(
        files.first().map(String::as_str),
        Some("app/Http/Controllers/RefundController.php"),
        "{out}"
    );
    assert!(files.len() <= 5, "{out}");
    assert!(
        files.contains(&"app/Services/RefundService.php".to_owned()),
        "{out}"
    );
    assert!(
        files.contains(&"resources/js/pages/checkout.vue".to_owned()),
        "{out}"
    );
    assert!(out.contains("class RefundController  L6-16"), "{out}");
    assert!(out.contains("(exact class)"), "{out}");
    assert!(out.contains("processRefund()  L6-9  callers: "), "{out}");
    assert!(out.contains("(exact method)"), "{out}");
    assert!(
        out.contains("file name  ← \"checkout.vue\" (file name)"),
        "{out}"
    );
    assert!(out.contains("★★★"), "{out}");
    // a gyökér egyszer, elöl; utána minden út relatív
    let root = project.0.display().to_string();
    assert!(out.lines().next().unwrap().starts_with("root: "), "{out}");
    assert_eq!(out.matches(&root).count(), 1, "{out}");
    // a hat találó fájlból öt látszik
    assert!(out.contains("more: --all (6 files)"), "{out}");
    assert!(!out.contains("no match:"), "{out}");
}

#[test]
fn all_lists_every_matching_file() {
    let project = Project::refunds();
    let out = project.run(&[&TERMS[..], &["--all"]].concat(), true);
    let files = ranked(&out);
    assert_eq!(files.len(), 6, "{out}");
    // a csak az általános kifejezésre (név-részlettel) illő fájl a végén: a ritka kifejezés (`payment_status`)
    // stringje többet ér
    assert_eq!(
        files.last().map(String::as_str),
        Some("database/migrations/2024_01_10_add_refunds_table.php"),
        "{out}"
    );
    assert!(out.contains("'payment_status'  L8"), "{out}");
    assert!(out.contains("(string)"), "{out}");
    assert!(
        out.contains("add_refunds_table.php") && out.contains("(name part)"),
        "{out}"
    );
    assert!(!out.contains("more: --all"), "{out}");
}

#[test]
fn ignored_generated_and_binary_files_never_appear() {
    let project = Project::refunds();
    for graph in [true, false] {
        let out = project.run(&["refund", "processRefund", "--all"], graph);
        for name in ["docs/refund.md", "dist/refund.min.js", "assets/refund.js"] {
            assert!(!out.contains(name), "{name} (graph: {graph}):\n{out}");
        }
    }
}

#[test]
fn unknown_terms_are_listed_and_a_typo_free_term_still_matches() {
    let project = Project::refunds();
    let out = project.run(&["zzzunknownzzz", "RefundService"], true);
    assert!(out.contains("no match: zzzunknownzzz"), "{out}");
    assert_eq!(
        ranked(&out).first().map(String::as_str),
        Some("app/Services/RefundService.php"),
        "{out}"
    );
    let none = project.run(&["zzzunknownzzz"], true);
    assert!(
        none.starts_with("locate: 1 terms · no match in the project map"),
        "{none}"
    );
    assert!(ranked(&none).is_empty(), "{none}");
}

#[test]
fn a_term_in_too_many_files_is_called_too_general() {
    let project = Project::empty();
    for i in 0..32 {
        project.write(
            &format!("app/Items/Item{i}.php"),
            &format!(
                "<?php\nclass Item{i}\n{{\n    public function handle()\n    {{\n        return 1;\n    }}\n}}\n"
            ),
        );
    }
    let out = project.run(&["handle"], true);
    assert!(out.contains("too general"), "{out}");
    assert!(out.contains(": handle"), "{out}");
    assert_eq!(ranked(&out).len(), 5, "{out}");
    assert!(out.contains("more: --all (32 files)"), "{out}");
}

#[test]
fn the_project_graph_and_the_no_graph_path_give_the_same_answer() {
    let project = Project::refunds();
    let args = [&TERMS[..], &["--all"]].concat();
    let cold = project.run(&args, true);
    let warm = project.run(&args, true);
    let analyzed = project.run(&args, false);
    assert_eq!(cold, warm);
    assert_eq!(cold, analyzed);
    // egy szerkesztés a következő hívásban látszik (a térkép a változott fájlra frissül)
    project.write(
        "app/Services/PaymentGateway.php",
        "<?php\nclass PaymentGateway\n{\n    public function chargeRefund()\n    {\n        return 'payment_status';\n    }\n}\n",
    );
    let after = project.run(&["PaymentGateway"], true);
    assert_eq!(
        ranked(&after).first().map(String::as_str),
        Some("app/Services/PaymentGateway.php"),
        "{after}"
    );
}

#[test]
fn usage_errors() {
    let project = Project::empty();
    for args in [&[][..], &["--all"][..], &["x", "--bogus"][..]] {
        let out = project.command(args, true);
        assert!(!out.status.success(), "{args:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("callgraph locate <term>"), "{args:?}: {err}");
    }
}
