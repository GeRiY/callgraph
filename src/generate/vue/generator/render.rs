//! A kifejezés kirajzolása: a szakaszok (script / template / style) és az importok, valamint a teljes új
//! `.vue` / composable / modul fájlszövegek.

use super::super::builder::{SfcBuilder, composable_file, script_body};
use super::super::edit::reindent;
use super::super::imports::ImportSet;
use super::super::model::{Element, Import, Program};
use super::super::project::CodeStyle;
use super::super::script::ScriptEmitter;
use super::super::template::components;
use super::super::{style, template};
use super::VueGenerator;
use super::script_edit::{composable_members, module_members};
use crate::generate::error::{GenError, GenResult};

/// A kirajzolt szakaszok és importok.
pub(super) struct Rendered {
    pub(super) script: Vec<String>,
    pub(super) template: Vec<String>,
    pub(super) style: Vec<String>,
    pub(super) imports: ImportSet,
    pub(super) unresolved: Vec<String>,
}

impl VueGenerator<'_> {
    /// A komponens-importok: a `bound` / explicit nevek kimaradnak; a többit a projekt-index oldja fel.
    pub(super) fn component_imports(
        &self,
        elements: &[Element],
        explicit: &[Import],
        bound: &[String],
    ) -> (Vec<Import>, Vec<String>) {
        let project = self.project();
        let (mut imports, mut unresolved) = (Vec::new(), Vec::new());
        for name in components(elements) {
            let known = bound.contains(&name)
                || explicit.iter().any(|i| {
                    i.default.as_deref() == Some(name.as_str()) || i.named.contains(&name)
                });
            if known {
                continue;
            }
            if let Some(source) = project.find_component(&name) {
                imports.push(Import {
                    source,
                    default: Some(name),
                    named: Vec::new(),
                });
            } else if matches!(name.as_str(), "Link" | "Head") {
                imports.push(Import {
                    source: "@inertiajs/vue3".to_owned(),
                    default: None,
                    named: vec![name],
                });
            } else {
                unresolved.push(name);
            }
        }
        (imports, unresolved)
    }

    /// A `use:<út>` importok: ahol a célfájl a projektben megvan, nincs `export default`-ja, de van a fájlnévvel
    /// azonos nevesített exportja (`export function useX`), ott nevesített import lesz a default helyett.
    pub(super) fn resolve_uses(&self, imports: &[Import], guess: &[String]) -> Vec<Import> {
        let project = self.project();
        imports
            .iter()
            .map(|import| {
                let named_export = import
                    .default
                    .as_ref()
                    .filter(|_| guess.contains(&import.source) && import.named.is_empty());
                match named_export {
                    Some(name) if project.exports_named_only(&import.source, name) => Import {
                        source: import.source.clone(),
                        default: None,
                        named: vec![name.clone()],
                    },
                    _ => import.clone(),
                }
            })
            .collect()
    }

    pub(super) fn render(&self, program: &Program, style: CodeStyle, bound: &[String]) -> Rendered {
        let em = ScriptEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        let mut imports = ImportSet::new();
        let mut script = Vec::new();
        let mut explicit: Vec<Import> = Vec::new();
        if let Some(s) = &program.script {
            script = em.members(&s.members);
            explicit = self.resolve_uses(&s.imports, &s.guess);
            imports.extend(explicit.iter().cloned());
            imports.extend(ScriptEmitter::needed_imports(&s.members));
        }
        let mut unresolved = Vec::new();
        let mut template_lines = Vec::new();
        if let Some(elements) = &program.template {
            let (found, missing) = self.component_imports(elements, &explicit, bound);
            imports.extend(found);
            unresolved = missing;
            template_lines = template::lines(elements);
        }
        let style_lines = program
            .style
            .as_ref()
            .map(|s| style::lines(&s.rules))
            .unwrap_or_default();
        Rendered {
            script,
            template: template_lines,
            style: style_lines,
            imports,
            unresolved,
        }
    }

    /// Egy kész `.vue` oldal a preset számára: az `sfc_file` szövege a projekt behúzás-egységében (`unit`).
    /// A komponenseknek a modellben explicit importjuk van, ezért a projekt-keresés nem fut.
    pub(crate) fn sfc_page(&self, program: &Program, style: CodeStyle, unit: usize) -> String {
        let (text, _) = self.sfc_file(program, style);
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut out = reindent(&lines, unit).join("\n");
        out.push('\n');
        out
    }

    /// A teljes új `.vue` fájl.
    pub(super) fn sfc_file(&self, program: &Program, style: CodeStyle) -> (String, Vec<String>) {
        let rendered = self.render(program, style, &[]);
        let mut builder = SfcBuilder::new();
        if program.script.is_some() || !rendered.imports.is_empty() {
            builder = builder.script(
                style.ts,
                script_body(&rendered.imports.lines(style.semi), &rendered.script),
            );
        }
        if program.template.is_some() {
            builder = builder.template(rendered.template);
        }
        if let Some(s) = &program.style {
            builder = builder.style(s.scoped, rendered.style);
        }
        (builder.build(), rendered.unresolved)
    }

    /// A teljes új composable-fájl.
    pub(super) fn composable_text(
        &self,
        program: &Program,
        rel: &str,
        style: CodeStyle,
    ) -> GenResult<String> {
        let members = composable_members(program)?;
        let name = match &program.name {
            Some(n) => n.clone(),
            None => {
                let file = rel.rsplit('/').next().unwrap_or(rel);
                let stem = file.rsplit_once('.').map_or(file, |(s, _)| s).to_owned();
                crate::generate::head::is_ident(&stem)
                    .then_some(stem)
                    .ok_or_else(|| {
                        GenError::new(
                            crate::t!(
                                "name the composable: `comp:useCart > …`",
                                "nevezd el a composable-t: `comp:useCart > …`"
                            )
                            .to_owned(),
                        )
                    })?
            }
        };
        let rendered = self.render(program, style, &[]);
        let em = ScriptEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        Ok(composable_file(
            &rendered.imports.lines(style.semi),
            &em.composable(&name, members),
        ))
    }

    /// A teljes új modul-fájl: importok, majd a tagok `export`-tal (JS-ben JSDoc-kal).
    pub(super) fn module_text(&self, program: &Program, style: CodeStyle) -> GenResult<String> {
        let members = module_members(program)?;
        let rendered = self.render(program, style, &[]);
        let em = ScriptEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        Ok(composable_file(
            &rendered.imports.lines(style.semi),
            &em.module(members),
        ))
    }
}
