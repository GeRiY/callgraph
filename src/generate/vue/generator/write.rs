//! Az új szöveg kiírása: az előnézet (cél nélkül) és a `--write`.

use super::super::check::check_script_file;
use super::super::model::{Kind, Program};
use super::super::project::CodeStyle;
use super::{FileKind, VueGenerator, file_kind, problem};
use crate::generate::command::{EditCommand, WriteNew};
use crate::generate::error::{GenError, GenResult};
use crate::generate::generator::{Mode, Output};

impl VueGenerator<'_> {
    pub(super) fn preview(&self, program: &Program) -> GenResult<Output> {
        let style = self.project().code_style();
        let as_module = program.module_like();
        let (text, unresolved) = match program.kind {
            Kind::Module => (self.module_text(program, style)?, Vec::new()),
            Kind::Composable => (
                self.composable_text(program, "preview.ts", style)?,
                Vec::new(),
            ),
            Kind::Sfc => self.sfc_file(program, style),
            Kind::Parts => {
                if as_module {
                    (self.module_text(program, style)?, Vec::new())
                } else {
                    if program.template.is_none() && program.style.is_none() {
                        return self.preview_members(program, style);
                    }
                    self.sfc_file(program, style)
                }
            }
        };
        let script_file = matches!(program.kind, Kind::Composable | Kind::Module) || as_module;
        let rel = if script_file {
            if style.ts { "preview.ts" } else { "preview.js" }
        } else {
            "preview.vue"
        };
        self.validate(rel, &text)?;
        Ok(Output {
            lines: text.lines().map(str::to_owned).collect(),
            unresolved,
        })
    }

    /// Csupasz tagok előnézete: a tagok sorai (egy ideiglenes függvénybe csomagolva ellenőrizve).
    pub(super) fn preview_members(&self, program: &Program, style: CodeStyle) -> GenResult<Output> {
        let rendered = self.render(program, style, &[]);
        let wrapped = format!(
            "function generated() {{\n{}\n}}\n",
            rendered.script.join("\n")
        );
        let path = if style.ts { "preview.ts" } else { "preview.js" };
        check_script_file(path, &wrapped).map_err(|mut p| {
            p.line = p.line.saturating_sub(1).max(1);
            problem(&p)
        })?;
        Ok(Output {
            lines: rendered.script,
            unresolved: rendered.unresolved,
        })
    }

    pub(super) fn write(&self, program: &Program, file: &str, mode: Mode) -> GenResult<Output> {
        let rel = self.rel(file)?;
        let kind = file_kind(&rel)?;
        let project = self.project().code_style();
        let (text, unresolved) = match kind {
            FileKind::Vue => {
                if program.kind == Kind::Composable {
                    return Err(GenError::new(
                        crate::t!(
                            "`comp:` writes a composable: use a .ts / .js file (a .vue file takes `vue:Name > script + template + style`)",
                            "a `comp:` composable-t ír: .ts / .js fájl kell (a .vue fájlhoz `vue:Név > script + template + style` való)"
                        )
                        .to_owned(),
                    ));
                }
                if program.kind == Kind::Module {
                    return Err(GenError::new(
                        crate::t!(
                            "`mod:` writes a module: use a .ts / .js file (a .vue file takes `vue:Name > script + template + style`)",
                            "a `mod:` modult ír: .ts / .js fájl kell (a .vue fájlhoz `vue:Név > script + template + style` való)"
                        )
                        .to_owned(),
                    ));
                }
                self.sfc_file(program, project)
            }
            FileKind::Script { ts } => {
                if program.kind == Kind::Sfc {
                    return Err(GenError::new(
                        crate::t!(
                            "`vue:` writes a .vue file: use `comp:useName > …` for a .ts / .js composable",
                            "a `vue:` .vue fájlt ír: .ts / .js composable-hoz `comp:useNév > …` kell"
                        )
                        .to_owned(),
                    ));
                }
                let style = CodeStyle { ts, ..project };
                let text = if program.is_module() {
                    self.module_text(program, style)?
                } else {
                    self.composable_text(program, &rel, style)?
                };
                (text, Vec::new())
            }
        };
        self.validate(&rel, &text)?;
        if mode.dry() {
            return Ok(Output {
                lines: text.lines().map(str::to_owned).collect(),
                unresolved,
            });
        }
        let line = WriteNew { rel, text }.execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved,
        })
    }
}
