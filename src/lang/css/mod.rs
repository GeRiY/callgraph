//! CSS frontend: Raffia-validálás és CSS → `FileGraph` leképezés.

mod frontend;
mod scanner;

pub use frontend::CssFrontend;

#[cfg(test)]
mod tests {
    use super::CssFrontend;
    use crate::core::SourceFile;
    use crate::lang::{Language, LanguageFrontend};

    #[test]
    fn analyzes_stylesheet_and_resolves_import() {
        let f = CssFrontend::new();
        let a = f
            .analyze(&SourceFile::new(
                "/tmp/css/theme.css",
                br#"@import url("print.css");
.card { color: var(--brand); background: rgba(0, 0, 0, .2); }
"#
                .to_vec(),
                None,
            ))
            .unwrap();
        assert_eq!(a.graph.containers[0].name, "theme");
        assert_eq!(a.graph.methods[0].body, None);
        assert_eq!(a.graph.imports.get("print.css"), Some("print.css"));
        assert!(a.graph.calls.iter().any(|c| c.target == "var"));
        assert_eq!(
            f.resolve_module("print.css", std::path::Path::new("/tmp/css/theme.css")),
            Some(std::path::PathBuf::from("/tmp/css/print.css"))
        );
    }
}
