//! A `.vue` fájl szakaszainak szerkesztése: a script / template / style beszúrása, az importok egyesítése.

use super::super::builder::{script_open, style_open};
use super::super::edit::{
    Doc, declared_names, find_statement, indent_block, indent_of, indent_unit, last_nonblank,
    reindent,
};
use super::super::imports::{ImportSet, plan, scan};
use super::super::model::{Member, Program};
use super::super::project::CodeStyle;
use super::super::template::opens_element;
use super::render::Rendered;
use crate::generate::error::{GenError, GenResult};
use crate::lang::js::{Sfc, SfcBlockKind};

fn one_line_block(name: &str) -> GenError {
    GenError::new(crate::tf!(
        "the <{}> block must have its tags on their own lines",
        "a <{}> blokk tagjai külön sorban legyenek",
        name
    ))
}

/// `--into x.vue`: a szakaszok beszúrása, majd az importok egyesítése. A generált kódsorokat adja vissza (`--dry`).
pub(super) fn insert_vue(
    doc: &mut Doc,
    program: &Program,
    rendered: &Rendered,
    style: CodeStyle,
    after: Option<&str>,
) -> GenResult<Vec<String>> {
    let after_line = after.and_then(parse_line_ref);
    let after_name = after.filter(|_| after_line.is_none());
    if after_line.is_some() && program.template.is_none() {
        return Err(GenError::new(
            crate::t!(
                "--after L<n> places template elements: the expression needs a `template>` section",
                "az --after L<n> template-elemeket helyez el: a kifejezésben `template>` szakasz kell"
            )
            .to_owned(),
        ));
    }
    // a meglévő fájl behúzás-egysége (2 vagy 4 szóköz) szerint
    let unit = indent_unit(&doc.lines);
    let mut shown = Vec::new();
    if program.template.is_some() {
        let block = reindent(&rendered.template, unit);
        put_template(doc, &block, after_line)?;
        shown.extend(block);
    }
    if program.style.is_some() {
        let block = reindent(&rendered.style, unit);
        put_style(
            doc,
            &block,
            program.style.as_ref().is_some_and(|s| s.scoped),
        );
        shown.extend(block);
    }
    let members = program
        .script
        .as_ref()
        .map_or(&[][..], |s| s.members.as_slice());
    if !members.is_empty() {
        let block = reindent(&rendered.script, unit);
        put_script(doc, members, &block, style, after_name)?;
        shown.extend(block);
    }
    if !rendered.imports.is_empty() {
        put_imports(doc, &rendered.imports, style)?;
    }
    Ok(shown)
}

/// A fájl már importált (lokális) nevei.
pub(super) fn bound_names(lines: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    for f in scan(lines, 0, lines.len()) {
        names.extend(f.import.default);
        names.extend(
            f.import
                .named
                .iter()
                .map(|n| n.rsplit(" as ").next().unwrap_or(n).trim().to_owned()),
        );
    }
    names
}

/// `L12` / `12` → 12.
pub(in crate::generate::vue) fn parse_line_ref(text: &str) -> Option<usize> {
    text.trim()
        .trim_start_matches(['L', 'l'])
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
}

/// A blokk nyitó-tag sora és záró-tag sora (0-alapú); `None`, ha nincs ilyen blokk.
pub(super) fn block_range(
    doc: &Doc,
    kind: SfcBlockKind,
    name: &str,
) -> GenResult<Option<(usize, usize)>> {
    let sfc = Sfc::parse(&doc.text());
    let Some(b) = sfc.blocks.iter().find(|b| b.kind == kind) else {
        return Ok(None);
    };
    let (tag, close) = (b.start_line as usize - 1, b.end_line as usize - 1);
    let closes_own_line = doc
        .lines
        .get(close)
        .is_some_and(|l| l.trim_start().starts_with("</"));
    if close <= tag || !closes_own_line {
        return Err(one_line_block(name));
    }
    Ok(Some((tag, close)))
}

fn put_script(
    doc: &mut Doc,
    members: &[Member],
    block: &[String],
    style: CodeStyle,
    after: Option<&str>,
) -> GenResult<()> {
    let range = block_range(doc, SfcBlockKind::ScriptSetup, "script")?;
    let Some((tag, close)) = range else {
        let mut ins = vec![script_open(style.ts).to_owned()];
        ins.extend(block.iter().cloned());
        ins.push("</script>".to_owned());
        ins.push(String::new());
        doc.insert(0, ins);
        doc.mark("script", 1, block.len());
        return Ok(());
    };
    let existing = doc.lines[tag + 1..close].join("\n");
    for m in members {
        let (macro_name, taken) = match m {
            Member::Props(_) => ("defineProps", existing.contains("defineProps")),
            Member::Emits(_) => ("defineEmits", existing.contains("defineEmits")),
            _ => ("", false),
        };
        if taken {
            return Err(GenError::new(crate::tf!(
                "the <script setup> already has {}",
                "a <script setup>-ban már van {}",
                macro_name
            )));
        }
        let clash = m
            .declares()
            .into_iter()
            .find(|name| find_statement(&doc.lines, name, tag + 1, close).is_some());
        if let Some(name) = clash {
            return Err(GenError::new(crate::tf!(
                "`{}` is already declared in the <script setup> (to change it use --replace <file>::{})",
                "a(z) `{}` már deklarálva van a <script setup>-ban (módosításhoz: --replace <fájl>::{})",
                name,
                name
            )));
        }
    }
    let at = match after {
        Some(name) => {
            let (_, end) = find_statement(&doc.lines, name, tag + 1, close).ok_or_else(|| {
                GenError::new(crate::tf!(
                    "no `{}` in the <script setup> (declared: {})",
                    "nincs `{}` a <script setup>-ban (deklarált: {})",
                    name,
                    declared_names(&doc.lines, tag + 1, close).join(", ")
                ))
            })?;
            end + 1
        }
        None => last_nonblank(&doc.lines, tag + 1, close).map_or(tag + 1, |i| i + 1),
    };
    let spaced = at > tag + 1 && !doc.lines[at - 1].trim().is_empty();
    let mut ins = Vec::new();
    if spaced {
        ins.push(String::new());
    }
    ins.extend(block.iter().cloned());
    if after.is_some() && at < close && !doc.lines[at].trim().is_empty() {
        ins.push(String::new());
    }
    doc.insert(at, ins);
    doc.mark("script", at + usize::from(spaced), block.len());
    Ok(())
}

fn put_template(doc: &mut Doc, block: &[String], after: Option<usize>) -> GenResult<()> {
    let range = block_range(doc, SfcBlockKind::Template, "template")?;
    let Some((tag, close)) = range else {
        let mut tpl = vec!["<template>".to_owned()];
        tpl.extend(indent_block(block, 2));
        tpl.push("</template>".to_owned());
        let sfc = Sfc::parse(&doc.text());
        let scripts_end = sfc.scripts().map(|b| b.end_line as usize).max();
        match scripts_end {
            Some(end) => {
                let mut ins = vec![String::new()];
                ins.extend(tpl);
                let len = block.len();
                doc.insert(end, ins);
                doc.mark("template", end + 2, len);
            }
            None => {
                tpl.push(String::new());
                doc.insert(0, tpl);
                doc.mark("template", 1, block.len());
            }
        }
        return Ok(());
    };
    let (at, indent) = match after {
        Some(n) => {
            if n <= tag || n > close {
                return Err(GenError::new(crate::tf!(
                    "--after L{} is outside the <template> (L{}-L{})",
                    "az --after L{} a <template>-en kívül van (L{}-L{})",
                    n,
                    tag + 1,
                    close + 1
                )));
            }
            let line = &doc.lines[n - 1];
            (n, indent_of(line) + if opens_element(line) { 2 } else { 0 })
        }
        None => match last_nonblank(&doc.lines, tag + 1, close) {
            None => (tag + 1, 2),
            Some(last) => {
                let line = &doc.lines[last];
                if line.trim_start().starts_with("</") {
                    (last, indent_of(line) + 2)
                } else {
                    (last + 1, indent_of(line))
                }
            }
        },
    };
    let ins = indent_block(block, indent);
    let len = ins.len();
    doc.insert(at, ins);
    doc.mark("template", at, len);
    Ok(())
}

fn put_style(doc: &mut Doc, block: &[String], scoped: bool) {
    let range = block_range(doc, SfcBlockKind::Style, "style")
        .ok()
        .flatten();
    let Some((tag, close)) = range else {
        while doc.lines.last().is_some_and(|l| l.trim().is_empty()) {
            doc.lines.pop();
        }
        if !doc.lines.is_empty() {
            doc.lines.push(String::new());
        }
        let start = doc.lines.len() + 1;
        doc.lines.push(style_open(scoped).to_owned());
        doc.lines.extend(block.iter().cloned());
        doc.lines.push("</style>".to_owned());
        doc.lines.push(String::new());
        doc.mark("style", start, block.len());
        return;
    };
    let last = last_nonblank(&doc.lines, tag + 1, close);
    let at = last.map_or(tag + 1, |i| i + 1);
    let mut ins = Vec::new();
    if last.is_some() {
        ins.push(String::new());
    }
    ins.extend(block.iter().cloned());
    doc.insert(at, ins);
    doc.mark("style", at + usize::from(last.is_some()), block.len());
}

pub(super) fn put_imports(doc: &mut Doc, imports: &ImportSet, style: CodeStyle) -> GenResult<()> {
    let range = block_range(doc, SfcBlockKind::ScriptSetup, "script")?;
    let Some((tag, close)) = range else {
        let mut ins = vec![script_open(style.ts).to_owned()];
        ins.extend(imports.lines(style.semi));
        ins.push("</script>".to_owned());
        ins.push(String::new());
        doc.insert(0, ins);
        return Ok(());
    };
    for e in plan(&doc.lines, tag + 1, close, imports, style.semi) {
        doc.splice(e.start, e.end, e.lines);
    }
    Ok(())
}
