//! `LanguageRegistry` (Factory): a regisztrált nyelvek közül fájlkiterjesztés szerint választ.

use std::path::Path;

use super::css::CssFrontend;
use super::go::GoFrontend;
use super::html::HtmlDocument;
use super::js::JsFrontend;
use super::json::JsonDocument;
use super::markdown::MdDocument;
use super::php::PhpBackend;
use super::python::PythonFrontend;
use super::rs::RustBackend;
use super::scss::ScssFrontend;
use super::swift::SwiftFrontend;
use super::{Language, LanguageBackend, LanguageDocument, LanguageFrontend};

/// A regisztrált backend-, frontend- és dokumentum-nyelvek. A keresés a regisztráció sorrendjében fut:
/// előbb a backendek, utána a frontendek, végül a dokumentum-nyelvek között.
#[derive(Default)]
pub struct LanguageRegistry {
    backends: Vec<Box<dyn LanguageBackend>>,
    frontends: Vec<Box<dyn LanguageFrontend>>,
    documents: Vec<Box<dyn LanguageDocument>>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Az alapértelmezett összeállítás: backend `PhpBackend`, `RustBackend`; frontend `CssFrontend`, `ScssFrontend`, `JsFrontend`, `SwiftFrontend`, `PythonFrontend`, `GoFrontend`; dokumentum `JsonDocument`, `MdDocument`, `HtmlDocument`.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register_backend(Box::new(PhpBackend::with_defaults()));
        registry.register_backend(Box::new(RustBackend::with_defaults()));
        registry.register_frontend(Box::new(CssFrontend::new()));
        registry.register_frontend(Box::new(ScssFrontend::new()));
        registry.register_frontend(Box::new(JsFrontend::with_defaults()));
        registry.register_frontend(Box::new(SwiftFrontend::with_defaults()));
        registry.register_frontend(Box::new(PythonFrontend::with_defaults()));
        registry.register_frontend(Box::new(GoFrontend::with_defaults()));
        registry.register_document(Box::new(JsonDocument));
        registry.register_document(Box::new(MdDocument));
        registry.register_document(Box::new(HtmlDocument));
        registry
    }

    pub fn register_backend(&mut self, language: Box<dyn LanguageBackend>) {
        self.backends.push(language);
    }

    pub fn register_frontend(&mut self, language: Box<dyn LanguageFrontend>) {
        self.frontends.push(language);
    }

    pub fn register_document(&mut self, language: Box<dyn LanguageDocument>) {
        self.documents.push(language);
    }

    /// A fájl kiterjesztését kezelő nyelv (kis-/nagybetűtől függetlenül: `B.PHP` is PHP); ismeretlen
    /// kiterjesztésre `None`.
    pub fn for_path(&self, path: &Path) -> Option<&dyn Language> {
        let ext = super::filetype::extension_lower(path)?;
        self.languages()
            .find(|l| l.extensions().contains(&ext.as_str()))
    }

    /// Az összes regisztrált kiterjesztés (a bejáró ezt használja), regisztrációs sorrendben.
    pub fn extensions(&self) -> Vec<&'static str> {
        self.languages()
            .flat_map(|l| l.extensions().iter().copied())
            .collect()
    }

    /// A megadott nyelv-azonosítókhoz tartozó kiterjesztések, regisztrációs sorrendben.
    pub fn extensions_for(&self, ids: &[&str]) -> Vec<&'static str> {
        self.languages()
            .filter(|language| ids.contains(&language.id()))
            .flat_map(|language| language.extensions().iter().copied())
            .collect()
    }

    /// A regisztrált nyelvek azonosítói, regisztrációs sorrendben.
    pub fn language_ids(&self) -> Vec<&'static str> {
        self.languages().map(Language::id).collect()
    }

    fn languages(&self) -> impl Iterator<Item = &dyn Language> {
        let backends = self.backends.iter().map(|l| &**l as &dyn Language);
        let frontends = self.frontends.iter().map(|l| &**l as &dyn Language);
        let documents = self.documents.iter().map(|l| &**l as &dyn Language);
        backends.chain(frontends).chain(documents)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::core::SourceFile;
    use crate::error::Result;
    use crate::lang::{Analysis, LanguageRole};
    use crate::model::FileGraphBuilder;

    struct Stub {
        id: &'static str,
        role: LanguageRole,
        exts: &'static [&'static str],
    }

    impl Language for Stub {
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
            Ok(Analysis {
                graph: FileGraphBuilder::new(file.path_string()).build(),
                warnings: Vec::new(),
            })
        }
    }

    impl LanguageBackend for Stub {
        fn detect_framework(&self, _: &Path) -> Option<String> {
            None
        }
    }

    impl LanguageFrontend for Stub {
        fn resolve_module(&self, _: &str, _: &Path) -> Option<PathBuf> {
            None
        }
    }

    impl LanguageDocument for Stub {
        fn resolve_reference(&self, _: &str, _: &Path) -> Option<PathBuf> {
            None
        }
    }

    fn stub(id: &'static str, role: LanguageRole, exts: &'static [&'static str]) -> Stub {
        Stub { id, role, exts }
    }

    fn registry() -> LanguageRegistry {
        let mut r = LanguageRegistry::new();
        r.register_backend(Box::new(stub("py", LanguageRole::Backend, &["py"])));
        r.register_frontend(Box::new(stub("js", LanguageRole::Frontend, &["js", "mjs"])));
        r.register_document(Box::new(stub("md", LanguageRole::Document, &["md"])));
        r
    }

    fn id_of(r: &LanguageRegistry, path: &str) -> Option<&'static str> {
        r.for_path(Path::new(path)).map(|l| l.id())
    }

    #[test]
    fn defaults_register_php_json_markdown_html() {
        let r = LanguageRegistry::with_defaults();
        assert_eq!(
            r.extensions(),
            vec![
                "php", "rs", "css", "scss", "js", "mjs", "cjs", "ts", "tsx", "jsx", "vue", "swift",
                "py", "pyi", "go", "json", "md", "html", "htm"
            ]
        );
        let php = r.for_path(Path::new("src/A.php")).unwrap();
        assert_eq!((php.id(), php.role()), ("php", LanguageRole::Backend));
        let json = r.for_path(Path::new("package.json")).unwrap();
        assert_eq!((json.id(), json.role()), ("json", LanguageRole::Document));
        let md = r.for_path(Path::new("docs/README.md")).unwrap();
        assert_eq!((md.id(), md.role()), ("markdown", LanguageRole::Document));
        for path in ["docs/design.html", "docs/OLD.HTM"] {
            let html = r.for_path(Path::new(path)).unwrap();
            assert_eq!((html.id(), html.role()), ("html", LanguageRole::Document));
        }
    }

    #[test]
    fn selects_language_by_extension() {
        let r = registry();
        assert_eq!(id_of(&r, "a/b.py"), Some("py"));
        assert_eq!(id_of(&r, "a/b.mjs"), Some("js"));
    }

    #[test]
    fn unknown_or_missing_extension_is_none() {
        let r = registry();
        assert_eq!(id_of(&r, "a/b.rb"), None);
        assert_eq!(id_of(&r, "Makefile"), None);
        assert_eq!(id_of(&LanguageRegistry::new(), "a.php"), None);
    }

    #[test]
    fn extension_match_ignores_case() {
        // a `B.PY` ugyanaz, mint a `b.py` (Windowson / macOS-en gyakori a nagybetűs kiterjesztés)
        assert_eq!(id_of(&registry(), "a/B.PY"), Some("py"));
        assert_eq!(id_of(&registry(), "a/b.Md"), Some("md"));
        assert_eq!(id_of(&registry(), "a/B.RB"), None);
    }

    #[test]
    fn roles_follow_registration_kind() {
        let r = registry();
        let role = |p: &str| r.for_path(Path::new(p)).map(|l| l.role());
        assert_eq!(role("x.py"), Some(LanguageRole::Backend));
        assert_eq!(role("x.js"), Some(LanguageRole::Frontend));
        assert_eq!(role("x.md"), Some(LanguageRole::Document));
    }

    #[test]
    fn extensions_list_backends_frontends_documents_in_order() {
        assert_eq!(registry().extensions(), vec!["py", "js", "mjs", "md"]);
    }

    #[test]
    fn filters_extensions_by_language_id() {
        assert_eq!(
            registry().extensions_for(&["js", "md"]),
            vec!["js", "mjs", "md"]
        );
        assert_eq!(registry().language_ids(), vec!["py", "js", "md"]);
    }
}
