//! Integrációs teszt: három teszt-nyelv (backend `.tb`, frontend `.tf`, document `.td`) bizonyítja,
//! hogy egy új nyelv a közös rétegek módosítása nélkül, pusztán regisztrációval beilleszthető.

use std::fs;
use std::path::{Path, PathBuf};

use callgraph::analysis::Analyzer;
use callgraph::core::SourceFile;
use callgraph::error::{Result, CallgraphError};
use callgraph::io::FileWalker;
use callgraph::lang::{
    Analysis, Language, LanguageBackend, LanguageDocument, LanguageFrontend, LanguageRegistry,
    LanguageRole,
};
use callgraph::model::FileGraphBuilder;

/// Közös teszt-nyelv: azonosító, szerep, kiterjesztés.
struct TestLang {
    id: &'static str,
    role: LanguageRole,
    exts: &'static [&'static str],
}

impl Language for TestLang {
    fn id(&self) -> &'static str {
        self.id
    }
    fn role(&self) -> LanguageRole {
        self.role
    }
    fn extensions(&self) -> &'static [&'static str] {
        self.exts
    }
    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let mut builder = FileGraphBuilder::new(file.path_string());
        builder.framework(Some(self.id.to_string()));
        Ok(Analysis {
            graph: builder.build(),
            warnings: Vec::new(),
        })
    }
}

impl LanguageBackend for TestLang {
    fn detect_framework(&self, _: &Path) -> Option<String> {
        Some("test-fw".into())
    }
}

impl LanguageFrontend for TestLang {
    fn resolve_module(&self, spec: &str, from: &Path) -> Option<PathBuf> {
        Some(from.parent()?.join(format!("{spec}.tf")))
    }
}

impl LanguageDocument for TestLang {
    fn resolve_reference(&self, reference: &str, from: &Path) -> Option<PathBuf> {
        if reference.is_empty() {
            return None;
        }
        Some(from.parent()?.join(reference))
    }
}

fn lang(id: &'static str, role: LanguageRole, exts: &'static [&'static str]) -> TestLang {
    TestLang { id, role, exts }
}

fn registry() -> LanguageRegistry {
    let mut r = LanguageRegistry::new();
    r.register_backend(Box::new(lang("tb", LanguageRole::Backend, &["tb"])));
    r.register_frontend(Box::new(lang("tf", LanguageRole::Frontend, &["tf"])));
    r.register_document(Box::new(lang("td", LanguageRole::Document, &["td"])));
    r
}

/// Ideiglenes mappa; a teszt végén `fs::remove_dir_all`.
fn temp_tree(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("callgraph-languages-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub")).unwrap();
    for name in ["a.tb", "b.tf", "c.td", "d.txt", "sub/e.tb", "sub/f.php"] {
        fs::write(dir.join(name), "x").unwrap();
    }
    dir
}

#[test]
fn language_is_chosen_by_extension() {
    let r = registry();
    let id = |p: &str| r.for_path(Path::new(p)).map(|l| l.id());
    assert_eq!(id("x/a.tb"), Some("tb"));
    assert_eq!(id("x/a.tf"), Some("tf"));
    assert_eq!(id("x/a.td"), Some("td"));
    assert_eq!(id("x/a.txt"), None);
}

#[test]
fn roles_match_registration_kind() {
    let r = registry();
    let role = |p: &str| r.for_path(Path::new(p)).map(|l| l.role());
    assert_eq!(role("a.tb"), Some(LanguageRole::Backend));
    assert_eq!(role("a.tf"), Some(LanguageRole::Frontend));
    assert_eq!(role("a.td"), Some(LanguageRole::Document));
}

#[test]
fn extensions_cover_all_roles() {
    assert_eq!(registry().extensions(), vec!["tb", "tf", "td"]);
}

#[test]
fn role_specific_traits_work() {
    let b = lang("tb", LanguageRole::Backend, &["tb"]);
    let f = lang("tf", LanguageRole::Frontend, &["tf"]);
    let d = lang("td", LanguageRole::Document, &["td"]);
    let from = Path::new("/p/a.tf");
    assert_eq!(
        b.detect_framework(Path::new("/p")).as_deref(),
        Some("test-fw")
    );
    assert_eq!(f.resolve_module("m", from), Some(PathBuf::from("/p/m.tf")));
    assert_eq!(
        d.resolve_reference("r.td", from),
        Some(PathBuf::from("/p/r.td"))
    );
    assert_eq!(d.resolve_reference("", from), None);
}

#[test]
fn walker_visits_registered_extensions_only() {
    let dir = temp_tree("walker");
    let walker = FileWalker::for_registry(&registry());
    let mut warnings = Vec::new();
    let files = walker.collect(std::slice::from_ref(&dir), &mut |w| warnings.push(w));
    let root = fs::canonicalize(&dir).unwrap();
    let mut names: Vec<String> = files
        .iter()
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names, vec!["a.tb", "b.tf", "c.td", "sub/e.tb"]);
    assert!(warnings.is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn analyzer_dispatches_to_the_right_language() {
    let dir = temp_tree("analyzer");
    let analyzer = Analyzer::new(registry());
    for (name, id) in [("a.tb", "tb"), ("b.tf", "tf"), ("c.td", "td")] {
        let analysis = analyzer.analyze_path(&dir.join(name)).unwrap();
        assert_eq!(analysis.graph.framework.as_deref(), Some(id));
    }
    let err = analyzer.analyze_path(&dir.join("d.txt")).unwrap_err();
    assert!(matches!(err, CallgraphError::Unsupported(_)));
    fs::remove_dir_all(&dir).unwrap();
}
