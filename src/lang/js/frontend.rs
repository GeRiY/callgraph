//! A JS / TS / Vue frontend Facade-ja: SFC-felbontás, parsolás, passzok futtatása.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_allocator::Allocator;

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::imports::ImportsPass;
use super::lines::LineIndex;
use super::meta::{ClassContainerPass, apply_file_meta};
use super::parse::{parse_program, source_type_for};
use super::resolve::Resolver;
use super::sfc::{Sfc, SfcBlockKind};
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageFrontend, LanguageRole};
use crate::model::Container;

/// JavaScript, TypeScript és Vue SFC forrásokat a közös fájlgráfba fordító frontend.
#[derive(Default)]
pub struct JsFrontend {
    passes: Vec<Box<dyn JsPass>>,
    resolver: Arc<Resolver>,
}

impl JsFrontend {
    /// Az alapértelmezett összeállítás (a passzok a következő lépésekben kerülnek be).
    pub fn new() -> Self {
        Self::default()
    }

    /// Az alapértelmezett passzok (egy sor = egy pass; a sorrend a futás sorrendje).
    pub fn with_defaults() -> Self {
        let frontend = Self::new();
        let resolver = frontend.resolver.clone();
        frontend
            .with_pass(Box::new(ImportsPass::new(resolver)))
            .with_pass(Box::new(ClassContainerPass))
            .with_pass(Box::new(super::container_doc::ContainerDocPass))
            .with_pass(Box::new(super::methods::MethodsPass))
            .with_pass(Box::new(super::props::PropsPass))
            .with_pass(Box::new(super::constants::ConstantsPass))
            .with_pass(Box::new(super::strings::StringsPass))
            .with_pass(Box::new(super::comments::CommentsPass))
            .with_pass(Box::new(super::control_flow::ControlFlowPass))
            .with_pass(Box::new(super::calls::CallsPass::default()))
    }

    /// Egy elemzési passz hozzáadása (DI).
    pub fn with_pass(mut self, pass: Box<dyn JsPass>) -> Self {
        self.passes.push(pass);
        self
    }

    fn analyze_block(
        &self,
        path: &Path,
        src: &str,
        setup: bool,
        vue_lang: Option<&str>,
        lines: &LineIndex,
        out: &mut JsAnalysis,
    ) {
        let allocator = Allocator::default();
        let parsed = parse_program(&allocator, src, source_type_for(path, vue_lang));
        if let Some(first) = parsed.errors.first() {
            out.warnings.push(crate::tf!(
                "{}: JS syntax error: {first}",
                "{}: JS szintaktikai hiba: {first}",
                path.display()
            ));
        }
        let ctx = PassContext {
            path,
            source: src,
            lines,
            setup,
        };
        for pass in &self.passes {
            pass.run(&ctx, &parsed.program, out);
        }
    }
}

impl Language for JsFrontend {
    fn id(&self) -> &'static str {
        "js"
    }

    fn role(&self) -> LanguageRole {
        LanguageRole::Frontend
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["js", "mjs", "cjs", "ts", "tsx", "jsx", "vue"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let is_vue = file.path.extension().and_then(|e| e.to_str()) == Some("vue");
        let mut out = JsAnalysis::new(file.path_string());
        apply_file_meta(&mut out.builder, &self.resolver, &file.path, &text, is_vue);
        out.builder.container(Container {
            kind: if is_vue { "component" } else { "module" }.to_string(),
            name: file
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            line: 1,
            end_line: LineIndex::new(&text).line_count(),
            is_abstract: Some(false),
            is_final: Some(false),
            extends: Some(Vec::new()),
            implements: Some(Vec::new()),
            traits: Some(Vec::new()),
            doc: None,
        });
        if is_vue {
            let sfc = Sfc::parse(&text);
            for block in sfc.scripts() {
                let src = &text[block.start..block.end];
                let lines = LineIndex::with_base(src, block.start_line);
                let setup = block.kind == SfcBlockKind::ScriptSetup;
                let lang = block.lang.as_deref();
                self.analyze_block(&file.path, src, setup, lang, &lines, &mut out);
            }
            if let Some(template) = sfc.template() {
                let name = file
                    .path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.builder
                    .method(super::methods::template_method(&name, template));
            }
        } else {
            let lines = LineIndex::new(&text);
            self.analyze_block(&file.path, &text, false, None, &lines, &mut out);
        }
        out.builder.sort_methods_by_line();
        // a template a kész gráf metódusait és importjait olvassa, ezért az utolsó lépés
        let mut graph = out.builder.build();
        if is_vue {
            super::template::analyze_template(&mut graph, &text, &Sfc::parse(&text));
        }
        Ok(Analysis {
            graph,
            warnings: out.warnings,
        })
    }
}

impl LanguageFrontend for JsFrontend {
    /// Váz: a relatív specifikátor a `from` melletti útra oldódik; az alias-feloldás a következő lépésben jön.
    fn resolve_module(&self, spec: &str, from: &Path) -> Option<PathBuf> {
        spec.starts_with('.')
            .then(|| from.parent().map(|p| p.join(spec)))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, text: &str) -> Analysis {
        JsFrontend::with_defaults()
            .analyze(&SourceFile::new(path, text.as_bytes().to_vec(), None))
            .unwrap()
    }

    #[test]
    fn js_file_gets_module_container() {
        let a = run("/p/util.ts", "export const a: number = 1;\n");
        let c = &a.graph.containers[0];
        assert_eq!(
            (c.kind.as_str(), c.name.as_str(), c.end_line),
            ("module", "util", 2)
        );
        assert!(a.graph.framework.as_deref() == Some("js") && a.warnings.is_empty());
        assert_eq!(a.graph.module.as_deref(), Some("p/util.ts"));
    }

    #[test]
    fn vue_file_gets_component_container_and_parses_scripts() {
        let a = run(
            "/p/Card.vue",
            "<template><p/></template>\n<script setup lang=\"ts\">\nconst a: number = 1\n</script>\n",
        );
        assert_eq!(a.graph.containers[0].kind, "component");
        assert_eq!(a.graph.containers[0].name, "Card");
        assert!(a.warnings.is_empty());
    }

    #[test]
    fn syntax_error_is_warning() {
        assert_eq!(run("/p/x.js", "const = ;").warnings.len(), 1);
    }
}
