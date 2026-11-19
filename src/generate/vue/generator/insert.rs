//! Az `--into`: a `.vue` / `.ts` / `.js` fájl szakaszainak, tagjainak és osztálytagjainak beszúrása.

use super::super::check::Problem;
use super::super::edit::{Doc, indent_block, indent_of, indent_unit, reindent};
use super::super::html_scan::{col_of, find, line_of, parse_selector, scan as scan_html};
use super::super::imports::{ImportSet, plan};
use super::super::model::Program;
use super::super::parse::parse_class_members;
use super::super::script::ScriptEmitter;
use super::script_edit::{class_span_of, insert_module, insert_script, module_target};
use super::sfc::{block_range, bound_names, insert_vue};
use super::{FileKind, VueGenerator, file_kind, problem};
use crate::generate::command::{EditCommand, InsertInto};
use crate::generate::error::{GenError, GenResult};
use crate::generate::generator::{Mode, Output};
use crate::generate::report;
use crate::generate::target::Loaded;
use crate::lang::js::SfcBlockKind;

impl VueGenerator<'_> {
    /// `--into`: a `.vue` fájl `::template#id` célja (egy elem utolsó gyereke) vagy a sima beszúrás.
    pub(super) fn insert_vue_or_script(
        &self,
        program: &Program,
        file: &str,
        class: Option<&str>,
        after: Option<&str>,
        mode: Mode,
    ) -> GenResult<Output> {
        let selector = class
            .map(|c| c.strip_prefix("template").unwrap_or(c))
            .filter(|c| c.starts_with(['#', '.']))
            .filter(|_| file.ends_with(".vue"));
        match selector {
            Some(sel) if after.is_none() => {
                let anchor = self.template_anchor(file, sel)?;
                self.insert(program, file, Some(anchor.as_str()), mode)
            }
            Some(_) => Err(GenError::new(
                crate::t!(
                    "::template#id already names the place: drop --after",
                    "a ::template#id már megnevezi a helyet: hagyd el az --after-t"
                )
                .to_owned(),
            )),
            None => self.insert(program, file, after, mode),
        }
    }

    /// A `::template#id` elem zárótagja előtti sor `L<n>` alakban (az `--after L<n>` számolja így).
    pub(super) fn template_anchor(&self, file: &str, selector: &str) -> GenResult<String> {
        let rel = self.rel(file)?;
        let Loaded { lines, .. } = self.load(&rel)?;
        let doc = Doc::new(lines);
        let Some((tag, close)) = block_range(&doc, SfcBlockKind::Template, "template")? else {
            return Err(GenError::new(crate::tf!(
                "no <template> in {}",
                "nincs <template> ebben: {}",
                rel
            )));
        };
        let inner = doc.lines[tag + 1..close].join("\n");
        let spans = scan_html(&inner).map_err(|(off, message)| {
            problem(&Problem {
                section: rel.clone(),
                line: tag + 2 + line_of(&inner, off),
                message,
            })
        })?;
        let sel = parse_selector(selector).ok_or_else(|| {
            GenError::new(crate::tf!(
                "bad element selector `{}` (use #id, .class or tag)",
                "hibás elem-választó: `{}` (#id, .class vagy tag kell)",
                selector
            ))
        })?;
        let Some(idx) = find(&inner, &spans, &sel) else {
            return Err(GenError::new(crate::tf!(
                "no element matches `{}` in the <template> of {}",
                "nincs elem, ami illene: `{}` ebben: {} <template>",
                selector,
                rel
            )));
        };
        let span = &spans[idx];
        let Some((close_at, _)) = span.close else {
            return Err(GenError::new(crate::tf!(
                "`<{}>` has no closing tag: nothing can be appended inside it",
                "a(z) `<{}>` nincs lezárva: nem lehet bele beszúrni",
                span.name
            )));
        };
        let (open_line, close_line) = (line_of(&inner, span.open.0), line_of(&inner, close_at));
        let before = doc.lines[tag + 1 + close_line]
            .get(..col_of(&inner, close_at))
            .unwrap_or("");
        if open_line == close_line || !before.trim().is_empty() {
            return Err(GenError::new(crate::tf!(
                "`{}` must have its opening and closing tags on their own lines",
                "a(z) `{}` nyitó- és zárótagja külön sorban legyen",
                selector
            )));
        }
        Ok(format!("L{}", tag + 1 + close_line))
    }

    pub(super) fn insert(
        &self,
        program: &Program,
        file: &str,
        after: Option<&str>,
        mode: Mode,
    ) -> GenResult<Output> {
        let rel = self.rel(file)?;
        let kind = file_kind(&rel)?;
        let Loaded { text, lines, eol } = self.load(&rel)?;
        let style = self.style_of(kind, &text);
        let mut doc = Doc::new(lines);
        let bound = bound_names(&doc.lines);
        let rendered = self.render(program, style, &bound);
        let block = match kind {
            FileKind::Vue => insert_vue(&mut doc, program, &rendered, style, after)?,
            FileKind::Script { .. } if module_target(program, &doc.lines) => {
                insert_module(&mut doc, program, &rendered, style, after)?
            }
            FileKind::Script { .. } | FileKind::React { .. } => {
                insert_script(&mut doc, program, &rendered, style, after)?
            }
        };
        self.commit_insert(rel, &doc, eol, block, rendered.unresolved, mode)
    }

    /// A szerkesztett szöveg ellenőrzése, majd (nem `--dry`-nál) kiírása mentéssel.
    pub(in crate::generate::vue) fn commit_insert(
        &self,
        rel: String,
        doc: &Doc,
        eol: &str,
        block: Vec<String>,
        unresolved: Vec<String>,
        mode: Mode,
    ) -> GenResult<Output> {
        let text = doc.lines.join(eol);
        self.validate(&rel, &text)?;
        let places: Vec<String> = doc
            .placed
            .iter()
            .map(|p| format!("{} L{}-{}", p.label, p.start + 1, p.start + p.len))
            .collect();
        if mode.dry() {
            return Ok(Output {
                lines: report::with_header(report::sections_header(&rel, &places), block),
                unresolved,
            });
        }
        let summary = report::inserted_sections(&rel, &places);
        let line = InsertInto { rel, text, summary }.execute(self.env)?;
        Ok(Output {
            lines: vec![line],
            unresolved,
        })
    }

    /// `--into x.ts::Osztály [--after tag]`: osztálytagok az osztály törzsébe (a hely az oxc-span alapján).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn insert_class(
        &self,
        expr: &str,
        file: &str,
        class: &str,
        after: Option<&str>,
        upsert: bool,
        expect: Option<&str>,
        mode: Mode,
    ) -> GenResult<Output> {
        let parts = parse_class_members(expr)?;
        let rel = self.rel(file)?;
        let kind = file_kind(&rel)?;
        let Loaded { text, lines, eol } = self.load(&rel)?;
        let style = self.style_of(kind, &text);
        let mut doc = Doc::new(lines);
        let span = class_span_of(&rel, &doc, class)?;
        if span.open == span.close {
            return Err(GenError::new(crate::tf!(
                "the body of `{}` is on one line: put its braces on separate lines first",
                "a(z) `{}` törzse egy sorban van: előbb tedd külön sorba a kapcsos zárójeleit",
                class
            )));
        }
        for m in &parts.members {
            if span.members.iter().any(|x| x.name == m.name()) {
                if upsert && parts.members.len() == 1 {
                    return self.replace_member(expr, file, (class, m.name()), expect, mode);
                }
                return Err(GenError::new(crate::tf!(
                    "`{}` is already a member of {} (to change it use --replace <file>::{}::{})",
                    "a(z) `{}` már tagja a(z) {} osztálynak (módosításhoz: --replace <fájl>::{}::{})",
                    m.name(),
                    class,
                    class,
                    m.name()
                )));
            }
        }
        let em = ScriptEmitter {
            ts: style.ts,
            semi: style.semi,
        };
        let unit = indent_unit(&doc.lines);
        let body_indent = span.members.first().map_or_else(
            || indent_of(&doc.lines[span.start]) + unit,
            |m| indent_of(&doc.lines[m.start]),
        );
        let block = indent_block(
            &reindent(&em.class_members(&parts.members), unit),
            body_indent,
        );
        let at = match after {
            Some(name) => {
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
                found.end + 1
            }
            None => span.close,
        };
        let spaced = at > span.open + 1 && !doc.lines[at - 1].trim().is_empty();
        let mut ins = Vec::new();
        if spaced {
            ins.push(String::new());
        }
        ins.extend(block.iter().cloned());
        let next_is_member = doc
            .lines
            .get(at)
            .is_some_and(|l| !l.trim().is_empty() && !l.trim_start().starts_with('}'));
        if after.is_some() && next_is_member {
            ins.push(String::new());
        }
        doc.insert(at, ins);
        doc.mark("members", at + usize::from(spaced), block.len());
        let mut wanted = ImportSet::new();
        wanted.extend(self.resolve_uses(&parts.imports, &parts.guess));
        if !wanted.is_empty() {
            let end = doc.lines.len();
            for e in plan(&doc.lines, 0, end, &wanted, style.semi) {
                doc.splice(e.start, e.end, e.lines);
            }
        }
        self.commit_insert(rel, &doc, eol, block, Vec::new(), mode)
    }
}
