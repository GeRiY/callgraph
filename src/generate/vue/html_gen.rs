//! A HTML-generátor Facade-ja a `VueGenerator` mellett: `html:` fejű kifejezés vagy `.html` / `.htm` cél →
//! `html::HtmlProgram` → sorok → ellenőrzés (tag-egyensúly, inline CSS / JS) → parancs (`WriteNew` / `InsertInto` /
//! `ReplaceRange`, mentéssel). Meglévő fájlban a hely: a `<body>` vége, `--after L<n>`, vagy egy elem (`::#id`,
//! `::.class`, `::tag`) utolsó gyereke; semmi nem íródik, amíg a teljes új szöveg hibátlan.

use super::check::Problem;
use super::edit::{Doc, indent_block, indent_of, indent_unit, last_nonblank, reindent};
use super::generator::{Loaded, VueGenerator, check_expect, parse_line_ref, problem};
use super::html::{HtmlDoc, HtmlProgram, document_lines, lines as render_nodes, parse};
use super::html_scan::{
    Span, check_html, col_of, duplicate_ids, find, line_of, parse_selector, scan,
};
use super::template::opens_element;
use crate::generate::command::{EditCommand, ReplaceRange, WriteNew};
use crate::generate::error::{GenError, GenResult};
use crate::generate::generator::{GenRequest, Output, ReplaceAt, Target};
use crate::generate::report;

/// `.html` / `.htm` fájl-e.
pub fn is_html_file(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

/// A HTML-generátornak szól-e a kérés: `.html` / `.htm` cél, vagy (cél nélkül) `html:` / `html>` / `html[` fej.
pub fn wants_html(request: &GenRequest) -> bool {
    match &request.target {
        Target::Preview => {
            let e = request.expr.trim_start();
            ["html:", "html>", "html["].iter().any(|p| e.starts_with(p))
        }
        Target::Write { file } | Target::Into { file, .. } | Target::Replace { file, .. } => {
            is_html_file(file)
        }
    }
}

/// Hová kerül a blokk.
enum Place {
    /// A `line` (0-alapú) sor elé.
    At { line: usize, indent: usize },
    /// A `line` sort a `col` oszlopnál kettévágva (a záró tag nem a sor elején áll).
    Split {
        line: usize,
        col: usize,
        indent: usize,
        tail_indent: usize,
    },
}

fn document_needs_write() -> GenError {
    GenError::new(
        crate::t!(
            "an `html:` document is written with --write; --into / --replace take elements",
            "az `html:` dokumentumot a --write írja; az --into / --replace elemeket vár"
        )
        .to_owned(),
    )
}

fn with_warnings(mut lines: Vec<String>, text: &str, fresh: &str) -> Output {
    lines.extend(duplicate_ids(text, fresh));
    Output {
        lines,
        unresolved: Vec::new(),
    }
}

/// A meglévő fájl elemei, vagy a szintaktikai hiba a fájl nevével.
fn spans_of(rel: &str, text: &str) -> GenResult<Vec<Span>> {
    scan(text).map_err(|(off, message)| {
        problem(&Problem {
            section: rel.to_owned(),
            line: line_of(text, off) + 1,
            message,
        })
    })
}

fn selector_of(text: &str) -> GenResult<super::html_scan::Selector> {
    parse_selector(text).ok_or_else(|| {
        GenError::new(crate::tf!(
            "bad element selector `{}` (use #id, .class or tag)",
            "hibás elem-választó: `{}` (#id, .class vagy tag kell)",
            text
        ))
    })
}

impl VueGenerator<'_> {
    /// A HTML-kérés végrehajtása.
    pub(super) fn run_html(&self, request: &GenRequest) -> GenResult<Output> {
        let program = parse(&request.expr)?;
        match &request.target {
            Target::Preview => self.html_preview(&program),
            Target::Write { file } => self.html_write(program, file, request.dry),
            Target::Into { file, class, after } => self.html_into(
                program,
                file,
                class.as_deref(),
                after.as_deref(),
                request.dry,
            ),
            Target::Replace { file, at, expect } => self.html_replace(
                program,
                file,
                at,
                expect.as_deref(),
                (request.dry, request.opts.diff),
            ),
        }
    }

    fn html_preview(&self, program: &HtmlProgram) -> GenResult<Output> {
        let block = match program {
            HtmlProgram::Document(doc) => document_lines(doc, "Document"),
            HtmlProgram::Fragment(nodes) => render_nodes(nodes),
        };
        let text = block.join("\n");
        check_html(&text).map_err(|p| problem(&p))?;
        Ok(with_warnings(block, &text, &text))
    }

    fn html_write(&self, program: HtmlProgram, file: &str, dry: bool) -> GenResult<Output> {
        let rel = self.rel(file)?;
        let doc = match program {
            HtmlProgram::Document(doc) => *doc,
            HtmlProgram::Fragment(nodes) => HtmlDoc::wrap(nodes),
        };
        let name = rel.rsplit('/').next().unwrap_or(&rel);
        let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
        let block = document_lines(&doc, stem);
        let mut text = block.join("\n");
        text.push('\n');
        self.validate(&rel, &text)?;
        if dry {
            return Ok(with_warnings(block, &text, &text));
        }
        let line = WriteNew {
            rel,
            text: text.clone(),
        }
        .execute(self.env)?;
        Ok(with_warnings(vec![line], &text, &text))
    }

    fn html_into(
        &self,
        program: HtmlProgram,
        file: &str,
        target: Option<&str>,
        after: Option<&str>,
        dry: bool,
    ) -> GenResult<Output> {
        let HtmlProgram::Fragment(nodes) = program else {
            return Err(document_needs_write());
        };
        let rel = self.rel(file)?;
        let Loaded { lines, eol, .. } = self.load(&rel)?;
        let mut doc = Doc::new(lines);
        let unit = indent_unit(&doc.lines);
        let generated = reindent(&render_nodes(&nodes), unit);
        let text = doc.text();
        let spans = spans_of(&rel, &text)?;
        let place = placement(&doc, &text, &spans, (target, after), unit)?;
        let (start, block) = match place {
            Place::At { line, indent } => {
                let block = indent_block(&generated, indent);
                doc.insert(line, block.clone());
                (line, block)
            }
            Place::Split {
                line,
                col,
                indent,
                tail_indent,
            } => {
                let block = indent_block(&generated, indent);
                let source = doc.lines[line].clone();
                let (head, tail) = source.split_at(col.min(source.len()));
                let mut replacement = Vec::new();
                let mut start = line;
                if !head.trim().is_empty() {
                    replacement.push(head.trim_end().to_owned());
                    start += 1;
                }
                replacement.extend(block.iter().cloned());
                replacement.push(format!("{}{tail}", " ".repeat(tail_indent)));
                doc.splice(line, line + 1, replacement);
                (start, block)
            }
        };
        doc.mark("html", start, block.len());
        let mut out = self.commit_insert(rel, &doc, eol, block, Vec::new(), dry)?;
        out.lines
            .extend(duplicate_ids(&doc.text(), &generated.join("\n")));
        Ok(out)
    }

    fn html_replace(
        &self,
        program: HtmlProgram,
        file: &str,
        at: &ReplaceAt,
        expect: Option<&str>,
        (dry, diff): (bool, bool),
    ) -> GenResult<Output> {
        let HtmlProgram::Fragment(nodes) = program else {
            return Err(document_needs_write());
        };
        let rel = self.rel(file)?;
        let Loaded { lines, eol, .. } = self.load(&rel)?;
        let mut doc = Doc::new(lines);
        let (from, to, label) = match at {
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
                (a - 1, *b, None)
            }
            ReplaceAt::Method { name, .. } => element_lines(&doc, &rel, name)?,
        };
        check_expect(&doc.lines, &rel, from + 1, to, expect)?;
        let unit = indent_unit(&doc.lines);
        let indent = doc.lines.get(from).map_or(0, |l| indent_of(l));
        let block = indent_block(&reindent(&render_nodes(&nodes), unit), indent);
        let fresh = block.join("\n");
        let old = doc.lines[from..to].to_vec();
        doc.splice(from, to, block.clone());
        doc.mark("html", from, block.len());
        let new_text = doc.lines.join(eol);
        self.validate(&rel, &new_text)?;
        let new = (from + 1, from + block.len());
        let label = label.unwrap_or_default();
        if dry {
            let header = report::replace_header(&rel, (from + 1, to), new, &label);
            return Ok(with_warnings(
                report::replace_preview(header, &old, block, diff),
                &doc.text(),
                &fresh,
            ));
        }
        let summary = report::replaced(&rel, (from + 1, to), new, &label);
        let line = ReplaceRange {
            rel,
            text: new_text,
            summary,
        }
        .execute(self.env)?;
        Ok(with_warnings(vec![line], &doc.text(), &fresh))
    }
}

/// A beszúrás helye: `--after L<n>`, egy elem utolsó gyereke (`::#id`), vagy a `<body>` vége.
fn placement(
    doc: &Doc,
    text: &str,
    spans: &[Span],
    (target, after): (Option<&str>, Option<&str>),
    unit: usize,
) -> GenResult<Place> {
    if let Some(a) = after {
        let n = parse_line_ref(a).filter(|n| *n <= doc.lines.len());
        let Some(n) = n else {
            return Err(GenError::new(crate::tf!(
                "--after takes a line of the file here: L<n> (1-{})",
                "az --after itt a fájl egy sora: L<n> (1-{})",
                doc.lines.len()
            )));
        };
        let line = &doc.lines[n - 1];
        let indent = indent_of(line) + if opens_element(line) { unit } else { 0 };
        return Ok(Place::At { line: n, indent });
    }
    if let Some(t) = target {
        let sel = selector_of(t)?;
        let Some(idx) = find(text, spans, &sel) else {
            return Err(GenError::new(crate::tf!(
                "no element matches `{}` (use #id, .class or tag)",
                "nincs elem, ami illene: `{}` (#id, .class vagy tag)",
                t
            )));
        };
        return into_element(doc, text, &spans[idx], unit);
    }
    let container = ["body", "html"]
        .iter()
        .find_map(|n| spans.iter().find(|s| s.name == *n && s.close.is_some()));
    match container {
        Some(span) => into_element(doc, text, span, unit),
        None => Ok(Place::At {
            line: last_nonblank(&doc.lines, 0, doc.lines.len()).map_or(0, |l| l + 1),
            indent: 0,
        }),
    }
}

/// Az elem utolsó gyerekének helye (a záró tag elé); a behúzás a meglévő első gyerek behúzása.
fn into_element(doc: &Doc, text: &str, span: &Span, unit: usize) -> GenResult<Place> {
    let Some((close_at, _)) = span.close else {
        return Err(GenError::new(crate::tf!(
            "`<{}>` has no closing tag: nothing can be appended inside it",
            "a(z) `<{}>` nincs lezárva: nem lehet bele beszúrni",
            span.name
        )));
    };
    let open_line = line_of(text, span.open.0);
    let close_line = line_of(text, close_at);
    let open_indent = indent_of(&doc.lines[open_line]);
    let col = col_of(text, close_at);
    let close_text = &doc.lines[close_line];
    let before = close_text.get(..col).unwrap_or("");
    let indent = (open_line + 1..close_line)
        .find(|&k| !doc.lines[k].trim().is_empty())
        .map_or(open_indent + unit, |k| indent_of(&doc.lines[k]));
    if before.trim().is_empty() {
        Ok(Place::At {
            line: close_line,
            indent,
        })
    } else {
        Ok(Place::Split {
            line: close_line,
            col,
            indent,
            tail_indent: open_indent,
        })
    }
}

/// Egy elem sorai (`--replace x.html::#id`): az elemnek a saját soraiban kell állnia.
fn element_lines(
    doc: &Doc,
    rel: &str,
    selector: &str,
) -> GenResult<(usize, usize, Option<String>)> {
    let text = doc.text();
    let spans = spans_of(rel, &text)?;
    let sel = selector_of(selector)?;
    let Some(idx) = find(&text, &spans, &sel) else {
        return Err(GenError::new(crate::tf!(
            "no element matches `{}` in {}",
            "nincs elem, ami illene: `{}` ebben: {}",
            selector,
            rel
        )));
    };
    let span = &spans[idx];
    let end = span.close.map_or(span.open.1, |c| c.1);
    let (first, last) = (line_of(&text, span.open.0), line_of(&text, end - 1));
    let before = doc.lines[first]
        .get(..col_of(&text, span.open.0))
        .unwrap_or("");
    let after = doc.lines[last]
        .get(col_of(&text, end - 1) + 1..)
        .unwrap_or("");
    if !before.trim().is_empty() || !after.trim().is_empty() {
        return Err(GenError::new(crate::tf!(
            "`{}` shares its lines with other content: use --replace {}:L{}-L{}",
            "a(z) `{}` más tartalommal közös sorban van: használd: --replace {}:L{}-L{}",
            selector,
            rel,
            first + 1,
            last + 1
        )));
    }
    Ok((first, last + 1, Some(selector.to_owned())))
}
