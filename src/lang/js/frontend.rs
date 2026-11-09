//! A JS / TS / Vue frontend Facade-ja: SFC-felbontás, parsolás, passzok futtatása.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_allocator::Allocator;

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::imports::ImportsPass;
use super::lines::LineIndex;
use super::meta::{ClassContainerPass, apply_file_meta};
use super::parse::{parse_recovering, source_type_for};
use super::resolve::Resolver;
use super::sfc::{Sfc, SfcBlockKind};
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageFrontend, LanguageRole};
use crate::model::{Container, ParseError};

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
        let parsed = parse_recovering(&allocator, src, source_type_for(path, vue_lang));
        if let Some(first) = parsed.errors.first() {
            out.warnings.push(crate::tf!(
                "{}: JS syntax error: {}",
                "{}: JS szintaktikai hiba: {}",
                path.display(),
                first.message
            ));
        }
        // a sor a teljes fájlban (Vue: a blokk eltolásával); a fájlvégi hiba a hiányos sorra mutat
        for error in &parsed.errors {
            let line = lines.line_of(crate::core::clamp_to_content(src.as_bytes(), error.offset));
            out.builder.parse_error(ParseError::new(line, &error.message));
        }
        // végzetes hibánál a passzok az ép előtagot látják (`parse_recovering`)
        let ctx = PassContext {
            path,
            source: parsed.source,
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
        let is_vue = crate::lang::filetype::extension_lower(&file.path).as_deref() == Some("vue");
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
    fn object_literal_methods_are_callers_and_targets() {
        let a = run(
            "/p/api.js",
            "export const api = {\n  fetch() { return load() },\n  save: () => api.fetch(),\n}\nfunction load() {}\n",
        );
        let calls: Vec<(&str, &str, &str)> = a
            .graph
            .calls
            .iter()
            .map(|c| {
                (
                    c.caller.as_deref().unwrap_or(""),
                    c.target.as_str(),
                    c.category.as_str(),
                )
            })
            .collect();
        assert!(
            calls.contains(&("api.fetch", "load", "internal_helper")),
            "{calls:?}"
        );
        assert!(
            calls.contains(&("api.save", "api.fetch", "internal_helper")),
            "{calls:?}"
        );
        let save = a
            .graph
            .methods
            .iter()
            .find(|m| m.name == "api.save")
            .unwrap();
        assert_eq!((save.line, save.end_line), (3, Some(3)));
    }

    #[test]
    fn vue_template_expressions_yield_strings() {
        let a = run(
            "/p/Box.vue",
            "<template>\n  <p :title=\"ok ? 'All saved' : 'Not saved'\">{{ n > 1 ? `${n} songs left` : 'Last song' }}</p>\n  <button @click=\"notify('Undo Favorite')\">x</button>\n</template>\n<script setup>\nconst n = 1\nfunction notify(m) {}\n</script>\n",
        );
        let strings: Vec<(&str, u32)> = a
            .graph
            .strings
            .iter()
            .filter(|s| s.caller.as_deref() == Some("template"))
            .map(|s| (s.value.as_str(), s.line))
            .collect();
        for expected in [
            ("All saved", 2),
            ("Not saved", 2),
            ("${…} songs left", 2),
            ("Last song", 2),
            ("Undo Favorite", 3),
        ] {
            assert!(strings.contains(&expected), "{expected:?} in {strings:?}");
        }
    }

    #[test]
    fn classes_and_interfaces_record_extends_and_implements() {
        let a = run(
            "/p/x.ts",
            "interface A {}\nexport interface B extends A, ns.C {}\nclass D extends Base implements B, A {}\n",
        );
        let find = |name: &str| {
            a.graph
                .containers
                .iter()
                .find(|c| c.name == name)
                .unwrap_or_else(|| panic!("{name}: {:?}", a.graph.containers))
        };
        assert_eq!((find("A").kind.as_str(), find("A").line), ("interface", 1));
        assert_eq!(find("B").kind, "interface");
        assert_eq!(
            find("B").extends,
            Some(vec!["A".to_string(), "ns.C".to_string()])
        );
        assert_eq!(find("D").extends, Some(vec!["Base".to_string()]));
        assert_eq!(
            find("D").implements,
            Some(vec!["B".to_string(), "A".to_string()])
        );
    }

    #[test]
    fn vue_script_setup_ts_resolves_typed_receivers_and_keeps_template_calls() {
        let a = run(
            "/p/Player.vue",
            "<template><button @click=\"svc.load()\">x</button></template>\n<script setup lang=\"ts\">\nimport { Svc } from './svc'\nconst svc = new Svc()\nfunction go(): void { svc.load() }\n</script>\n",
        );
        let go = a
            .graph
            .calls
            .iter()
            .find(|c| c.caller.as_deref() == Some("go"))
            .unwrap_or_else(|| panic!("{:?}", a.graph.calls));
        assert_eq!(
            (
                go.target.as_str(),
                go.category.as_str(),
                go.resolved.as_deref()
            ),
            ("load", "module_use", Some("Svc::load"))
        );
        assert!(
            a.graph
                .calls
                .iter()
                .any(|c| c.caller.as_deref() == Some("template") && c.target.ends_with("load")),
            "{:?}",
            a.graph.calls
        );
    }

    #[test]
    fn syntax_error_is_warning() {
        assert_eq!(run("/p/x.js", "const = ;").warnings.len(), 1);
    }

    fn method_names(a: &Analysis) -> Vec<&str> {
        a.graph.methods.iter().map(|m| m.name.as_str()).collect()
    }

    #[test]
    fn fatal_syntax_error_keeps_the_intact_prefix_and_reports_the_line() {
        let src = "export function a() { return 1 }\nexport function b() { return 2 }\nexport function c( {\n";
        for path in ["/p/tail.js", "/p/tail.ts"] {
            let a = run(path, src);
            let names = method_names(&a);
            assert!(names.contains(&"a") && names.contains(&"b"), "{names:?}");
            let first = &a.graph.parse_errors[0];
            assert_eq!(first.line, 3, "{:?}", a.graph.parse_errors);
            assert!(!first.message.is_empty() && !first.message.contains('\n'));
        }
    }

    #[test]
    fn vue_script_error_line_is_relative_to_the_file() {
        let a = run(
            "/p/v.vue",
            "<template><div/></template>\n<script setup>\nfunction a() { return 1 }\nfunction b( {\n</script>\n",
        );
        let names = method_names(&a);
        assert!(names.contains(&"a"), "{names:?}");
        assert_eq!(a.graph.parse_errors[0].line, 4, "{:?}", a.graph.parse_errors);
    }

    #[test]
    fn valid_file_has_no_parse_errors() {
        let a = run("/p/ok.js", "export function a() { return 1 }\n");
        assert!(a.graph.parse_errors.is_empty());
        let json = serde_json::to_string(&a.graph).unwrap();
        assert!(!json.contains("parseErrors"));
    }
}
