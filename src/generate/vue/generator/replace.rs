//! A `--replace`: egy tartomány, függvény, szakasz vagy osztálytag cseréje.

use super::super::edit::{
    Doc, declared_names, find_statement, indent_block, indent_of, indent_unit, reindent,
};
use super::super::imports::{ImportSet, plan};
use super::super::model::{Member, Program};
use super::super::parse::parse_class_members;
use super::super::project::CodeStyle;
use super::super::script::{ScriptEmitter, is_doc_line};
use super::render::Rendered;
use super::script_edit::{class_span_of, composable_members, module_members, module_target};
use super::sfc::{block_range, bound_names, put_imports};
use super::{FileKind, VueGenerator, check_expect, file_kind};
use crate::generate::command::{EditCommand, ReplaceRange};
use crate::generate::edit::extend_over_docs;
use crate::generate::error::{GenError, GenResult};
use crate::generate::generator::{Mode, Output, ReplaceAt};
use crate::generate::report;
use crate::generate::target::Loaded;
use crate::lang::js::SfcBlockKind;

impl VueGenerator<'_> {
    /// `--replace x.ts::Osztály::tag`: egy osztálytag (és a JSDoc-ja) cseréje.
    pub(super) fn replace_member(
        &self,
        expr: &str,
        file: &str,
        (class, name): (&str, &str),
        expect: Option<&str>,
        mode: Mode,
    ) -> GenResult<Output> {
        let parts = parse_class_members(expr)?;
        if parts.members.len() != 1 {
            return Err(GenError::new(
                crate::t!(
                    "--replace takes exactly one class member",
                    "a --replace pontosan egy osztálytagot vár"
                )
                .to_owned(),
            ));
        }
        let rel = self.rel(file)?;
        let kind = file_kind(&rel)?;
        let Loaded { text, lines, eol } = self.load(&rel)?;
        let style = self.style_of(kind, &text);
        let mut doc = Doc::new(lines);
        let span = class_span_of(&rel, &doc, class)?;
        let found = span.members.iter().find(|m| m.name == name);
        let found = found.ok_or_else(|| {
            GenError::new(crate::tf!(
                "no `{}` in {} (members: {})",
                "nincs `{}` a(z) {} osztályban (tagok: {})",
                name,
                class,
                span.members
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        let (from, to) = (found.start, found.end + 1);
        check_expect(&doc.lines, &rel, from + 1, to, expect)?;
        let from = if style.ts {
            from
        } else {
            extend_over_docs(&doc.lines, from + 1) - 1
        };
        let em = ScriptEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        let indent = indent_of(&doc.lines[found.start]);
        let block = indent_block(
            &reindent(&em.class_members(&parts.members), indent_unit(&doc.lines)),
            indent,
        );
        let old = doc.lines[from..to].to_vec();
        doc.splice(from, to, block.clone());
        doc.mark("members", from, block.len());
        let mut wanted = ImportSet::new();
        wanted.extend(self.resolve_uses(&parts.imports, &parts.guess));
        if !wanted.is_empty() {
            let end = doc.lines.len();
            for e in plan(&doc.lines, 0, end, &wanted, style.semi) {
                doc.splice(e.start, e.end, e.lines);
            }
        }
        let new_text = doc.lines.join(eol);
        self.validate(&rel, &new_text)?;
        let new = (from + 1, from + block.len());
        let label = format!("{class}::{name}");
        if mode.dry() {
            let header = report::replace_header(&rel, (from + 1, to), new, &label);
            return Ok(Output {
                lines: report::replace_preview(header, &old, block, mode.diff()),
                unresolved: Vec::new(),
            });
        }
        let summary = report::replaced(&rel, (from + 1, to), new, &label);
        let line = ReplaceRange {
            rel,
            text: new_text,
            summary,
        }
        .execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved: Vec::new(),
        })
    }

    pub(super) fn replace(
        &self,
        program: &Program,
        file: &str,
        at: &ReplaceAt,
        expect: Option<&str>,
        mode: Mode,
    ) -> GenResult<Output> {
        let rel = self.rel(file)?;
        let kind = file_kind(&rel)?;
        let Loaded { text, lines, eol } = self.load(&rel)?;
        let style = self.style_of(kind, &text);
        let mut doc = Doc::new(lines);
        let (from, to, label) = replace_range(&doc, kind, at, &rel)?;
        check_expect(&doc.lines, &rel, from + 1, to, expect)?;
        let bound = bound_names(&doc.lines);
        let rendered = self.render(program, style, &bound);
        let module_mode =
            matches!(kind, FileKind::Script { .. }) && module_target(program, &doc.lines);
        let (section, block) = if module_mode {
            let exported = doc
                .lines
                .get(from)
                .is_some_and(|l| l.trim_start().starts_with("export "));
            self.module_replacement(program, style, exported)?
        } else {
            self.replacement_block(program, kind, &rendered)?
        };
        // a JS-modul új JSDoc-ja a régi helyére kerül (nem kétszereződik)
        let docs_first = module_mode
            && block
                .first()
                .is_some_and(|l| l.trim_start().starts_with("/**"));
        let from = if docs_first {
            extend_over_docs(&doc.lines, from + 1) - 1
        } else {
            from
        };
        let indent = doc.lines.get(from).map_or(0, |l| indent_of(l));
        let block = indent_block(&reindent(&block, indent_unit(&doc.lines)), indent);
        let old = doc.lines[from..to].to_vec();
        doc.splice(from, to, block.clone());
        doc.mark(section, from, block.len());
        match kind {
            FileKind::Vue => {
                if !rendered.imports.is_empty() {
                    put_imports(&mut doc, &rendered.imports, style)?;
                }
            }
            FileKind::Script { .. } => {
                let end = doc.lines.len();
                for e in plan(&doc.lines, 0, end, &rendered.imports, style.semi) {
                    doc.splice(e.start, e.end, e.lines);
                }
            }
        }
        let new_text = doc.lines.join(eol);
        self.validate(&rel, &new_text)?;
        let placed = doc
            .placed
            .iter()
            .find(|p| p.label == section)
            .map_or((from, block.len()), |p| (p.start, p.len));
        let new = (placed.0 + 1, placed.0 + placed.1);
        let label = label.unwrap_or_default();
        if mode.dry() {
            let header = report::replace_header(&rel, (from + 1, to), new, &label);
            return Ok(Output {
                lines: report::replace_preview(header, &old, block, mode.diff()),
                unresolved: rendered.unresolved,
            });
        }
        let summary = report::replaced(&rel, (from + 1, to), new, &label);
        let line = ReplaceRange {
            rel,
            text: new_text,
            summary,
        }
        .execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved: rendered.unresolved,
        })
    }

    /// Modul-tag cseréje: a sima tag megtartja az eredeti export-állapotot (`-` / `!` előtaggal felülírható).
    pub(super) fn module_replacement(
        &self,
        program: &Program,
        style: CodeStyle,
        exported: bool,
    ) -> GenResult<(&'static str, Vec<String>)> {
        let members = module_members(program)?;
        let em = ScriptEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        let mut lines = em.module(members);
        let plain = members.len() == 1 && !matches!(members[0], Member::Exported { .. });
        if plain && !exported {
            let at = lines.iter().position(|l| !is_doc_line(l));
            if let Some(at) = at {
                let stripped = lines[at].strip_prefix("export ").map(str::to_owned);
                if let Some(first) = stripped {
                    lines[at] = first;
                }
            }
        }
        Ok(("members", lines))
    }

    /// A csere tartalma: pontosan egy szakasz (script-tagok / template / style).
    pub(super) fn replacement_block(
        &self,
        program: &Program,
        kind: FileKind,
        rendered: &Rendered,
    ) -> GenResult<(&'static str, Vec<String>)> {
        if kind != FileKind::Vue {
            composable_members(program)?;
            return Ok(("members", rendered.script.clone()));
        }
        if program.sections() != 1 {
            return Err(GenError::new(
                crate::t!(
                    "--replace takes exactly one section (`script>…`, `template>…` or `style>…`)",
                    "a --replace pontosan egy szakaszt vár (`script>…`, `template>…` vagy `style>…`)"
                )
                .to_owned(),
            ));
        }
        Ok(if program.script.is_some() {
            ("script", rendered.script.clone())
        } else if program.template.is_some() {
            ("template", rendered.template.clone())
        } else {
            ("style", rendered.style.clone())
        })
    }
}

/// A kicserélendő `[from, to)` sortartomány (0-alapú) és a függvény neve, ha név alapján kerestük.
pub(super) fn replace_range(
    doc: &Doc,
    kind: FileKind,
    at: &ReplaceAt,
    rel: &str,
) -> GenResult<(usize, usize, Option<String>)> {
    match at {
        ReplaceAt::Lines(a, b) => {
            if *a == 0 || a > b || *b > doc.lines.len() {
                return Err(GenError::new(crate::tf!(
                    "bad line range L{}-{} ({} has {} lines)",
                    "hibás sortartomány: L{}-{} ({} {} soros)",
                    a,
                    b,
                    rel,
                    doc.lines.len()
                )));
            }
            Ok((a - 1, *b, None))
        }
        // `x.vue::script` / `::template` / `::style`: a szakasz teljes tartalma (a tagek nélkül)
        ReplaceAt::Method { name, .. }
            if kind == FileKind::Vue
                && matches!(name.as_str(), "script" | "template" | "style") =>
        {
            let candidates: &[SfcBlockKind] = match name.as_str() {
                "script" => &[SfcBlockKind::ScriptSetup, SfcBlockKind::Script],
                "template" => &[SfcBlockKind::Template],
                _ => &[SfcBlockKind::Style],
            };
            for candidate in candidates {
                if let Some((tag, close)) = block_range(doc, *candidate, name)? {
                    return Ok((tag + 1, close, Some(name.clone())));
                }
            }
            Err(GenError::new(crate::tf!(
                "no <{}> section in {}",
                "nincs <{}> szakasz ebben: {}",
                name,
                rel
            )))
        }
        ReplaceAt::Method { name, .. } => {
            let (lo, hi) = match kind {
                FileKind::Vue => {
                    let (tag, close) = block_range(doc, SfcBlockKind::ScriptSetup, "script")?
                        .ok_or_else(|| {
                            GenError::new(crate::tf!(
                                "no <script setup> in {}",
                                "nincs <script setup> ebben: {}",
                                rel
                            ))
                        })?;
                    (tag + 1, close)
                }
                FileKind::Script { .. } => (0, doc.lines.len()),
            };
            let (start, end) = find_statement(&doc.lines, name, lo, hi).ok_or_else(|| {
                GenError::new(crate::tf!(
                    "no `{}` in {} (declared: {})",
                    "nincs `{}` ebben: {} (deklarált: {})",
                    name,
                    rel,
                    declared_names(&doc.lines, lo, hi).join(", ")
                ))
            })?;
            Ok((start, end + 1, Some(name.clone())))
        }
    }
}
