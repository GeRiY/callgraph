//! `callgraph locate` és a property-k / mezők (valódi binárissal, a projekt-gráffal és nélküle): a PHP property
//! (`protected $fillable`, promoted constructor property) és a TS class field / parameter property pontos névvel
//! definíció, névrészlettel név-részlet; a sok modellben definiált `$fillable` nem nyomja el a pontos osztály- és a
//! ritka kifejezés találatát.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

/// Ennyi modell definiálja a `$fillable`-t az `Invoice` mellett.
const MODELS: usize = 20;

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "callgraph-locate-props-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let project = Self(root.canonicalize().unwrap());
        for n in 0..MODELS {
            project.write(
                &format!("app/Models/Model{n}.php"),
                &format!(
                    "<?php\nnamespace App\\Models;\n\nclass Model{n}\n{{\n    protected $fillable = ['name'];\n}}\n"
                ),
            );
        }
        project.write(
            "app/Models/Invoice.php",
            "<?php\nnamespace App\\Models;\n\nclass Invoice\n{\n    protected $fillable = ['total'];\n    protected $guardedColumns = [];\n\n    public function __construct(private InvoiceRepository $ledgerStore)\n    {\n    }\n}\n",
        );
        project.write(
            "app/Http/Controllers/RefundController.php",
            "<?php\nnamespace App\\Http\\Controllers;\n\nclass RefundController\n{\n    public function store()\n    {\n        return 1;\n    }\n}\n",
        );
        project.write(
            "resources/js/Cart.ts",
            "export class Cart {\n  private itemCount: number = 0;\n  #secretToken = '';\n  constructor(private readonly cartRepository: Repo) {}\n  total(): number {\n    return this.itemCount;\n  }\n}\n",
        );
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
            .env("CALLGRAPH_STATS", "0")
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
        String::from_utf8(out.stdout).unwrap().replace('\\', "/")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A rangsor fájl-sorai (`1. út  ★★★`).
fn ranked(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| line.split_once(". "))
        .filter(|(n, _)| n.parse::<usize>().is_ok())
        .map(|(_, rest)| rest.split("  ").next().unwrap_or(rest).to_owned())
        .collect()
}

#[test]
fn an_exact_php_property_name_is_a_definition() {
    let project = Project::new();
    for graph in [true, false] {
        for term in ["fillable", "$fillable", "Invoice::$fillable"] {
            let out = project.run(&[term, "--all"], graph);
            assert!(!out.contains("no match"), "{graph} {term}: {out}");
            assert!(
                out.contains(&format!(
                    "property $fillable  L6  ← \"{term}\" (exact property)"
                )),
                "{graph} {term}: {out}"
            );
            assert!(
                ranked(&out).contains(&"app/Models/Invoice.php".to_owned()),
                "{graph} {term}: {out}"
            );
        }
        // a `Típus::$név` alak csak a saját típusára illik
        let out = project.run(&["Invoice::$fillable", "--all"], graph);
        assert_eq!(ranked(&out), ["app/Models/Invoice.php"], "{graph}: {out}");
        // a promoted constructor property is
        let out = project.run(&["ledgerStore"], graph);
        assert!(
            out.contains("property $ledgerStore  L9  ← \"ledgerStore\" (exact property)"),
            "{graph}: {out}"
        );
    }
}

#[test]
fn a_property_name_part_is_a_name_part_match() {
    let project = Project::new();
    for graph in [true, false] {
        let out = project.run(&["guarded"], graph);
        assert_eq!(ranked(&out), ["app/Models/Invoice.php"], "{graph}: {out}");
        assert!(
            out.contains("property $guardedColumns  L7  ~ \"guarded\" (name part)"),
            "{graph}: {out}"
        );
        // a szó eleje nem név-részlet: nincs találat
        let out = project.run(&["guard"], graph);
        assert!(out.contains("no match: guard"), "{graph}: {out}");
    }
}

#[test]
fn typescript_class_fields_are_properties() {
    let project = Project::new();
    for graph in [true, false] {
        for (term, line) in [
            (
                "itemCount",
                "property itemCount  L2  ← \"itemCount\" (exact property)",
            ),
            (
                "secretToken",
                "property #secretToken  L3  ← \"secretToken\" (exact property)",
            ),
            (
                "cartRepository",
                "property cartRepository  L4  ← \"cartRepository\" (exact property)",
            ),
        ] {
            let out = project.run(&[term], graph);
            assert!(
                out.contains("1. resources/js/Cart.ts"),
                "{graph} {term}: {out}"
            );
            assert!(out.contains(line), "{graph} {term}: {out}");
        }
    }
}

#[test]
fn a_property_defined_in_many_models_does_not_bury_other_matches() {
    let project = Project::new();
    for graph in [true, false] {
        // a pontos osztály-találat a sok `$fillable` előtt
        let out = project.run(&["Invoice", "fillable"], graph);
        assert_eq!(
            ranked(&out).first().map(String::as_str),
            Some("app/Models/Invoice.php"),
            "{graph}: {out}"
        );
        // a ritka kifejezés név-részlete (RefundController) a sok modell gyakori property-je előtt
        let out = project.run(&["refund", "fillable"], graph);
        assert_eq!(
            ranked(&out).first().map(String::as_str),
            Some("app/Http/Controllers/RefundController.php"),
            "{graph}: {out}"
        );
        // a modellek közben megtalálhatók (a lista többi része)
        let out = project.run(&["refund", "fillable", "--all"], graph);
        assert_eq!(ranked(&out).len(), MODELS + 2, "{graph}: {out}");
    }
}
