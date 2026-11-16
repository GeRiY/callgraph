//! A `.ts` / `.js` fájlok szerkesztése: modul-tagok és composable-tagok beszúrása, a `return { … }` bővítése,
//! az osztály helye.

use super::super::edit::{
    Doc, declared_names, find_statement, indent_block, indent_of, indent_unit, last_nonblank,
    reindent,
};
use super::super::imports::plan;
use super::super::locate::{ClassSpan, class_names, find_class};
use super::super::model::{Member, Program};
use super::super::project::CodeStyle;
use super::super::script::ScriptEmitter;
use super::render::Rendered;
use crate::generate::error::{GenError, GenResult};

/// Az osztály helye a fájlban, vagy hiba a meglévő osztályok felsorolásával.
pub(super) fn class_span_of(rel: &str, doc: &Doc, class: &str) -> GenResult<ClassSpan> {
    let text = doc.text();
    find_class(rel, &text, class).ok_or_else(|| {
        GenError::new(crate::tf!(
            "no class `{}` in {} (classes: {})",
            "nincs `{}` osztály ebben: {} (osztályok: {})",
            class,
            rel,
            class_names(rel, &text).join(", ")
        ))
    })
}

/// Modulként kezelendő-e a kifejezés a meglévő fájlnál: `mod:` / osztály / export-módosító, vagy Vue-specifikus
/// tag nélküli csupasz tagok egy nem composable fájlba.
pub(super) fn module_target(program: &Program, lines: &[String]) -> bool {
    program.module_like() || (program.is_module() && !is_composable_file(lines))
}

/// Composable-fájl-e: van `return {`, és a legfelső szintű, azt tartalmazó deklaráció neve `use…`.
pub(super) fn is_composable_file(lines: &[String]) -> bool {
    let Some(ret) = find_return(lines) else {
        return false;
    };
    let top = lines[..ret]
        .iter()
        .rev()
        .find(|l| !l.trim().is_empty() && indent_of(l) == 0);
    top.is_some_and(|l| {
        ["function use", "const use", "let use"]
            .iter()
            .any(|p| l.contains(p))
    })
}

/// A modul tagjai: csak script-tagok (Vue-specifikus tag nélkül).
pub(super) fn module_members(program: &Program) -> GenResult<&[Member]> {
    if program.template.is_some() || program.style.is_some() {
        return Err(GenError::new(
            crate::t!(
                "a .ts / .js file takes members only (no template / style): use a .vue file for those",
                "a .ts / .js fájl csak tagokat vár (template / style nélkül): azokhoz .vue fájl való"
            )
            .to_owned(),
        ));
    }
    let members = program
        .script
        .as_ref()
        .map_or(&[][..], |s| s.members.as_slice());
    if members.is_empty() {
        return Err(GenError::new(
            crate::t!(
                "the expression has no members (e.g. `mod:util>fn:slug(s:str):str{ … }`)",
                "a kifejezésben nincs tag (pl. `mod:util>fn:slug(s:str):str{ … }`)"
            )
            .to_owned(),
        ));
    }
    if members.iter().any(Member::is_vue_only) {
        return Err(GenError::new(
            crate::t!(
                "props / emits / ref / computed / form / on: belong to a Vue component or a `comp:` composable, not to a module",
                "a props / emits / ref / computed / form / on: Vue-komponensbe vagy `comp:` composable-be való, nem modulba"
            )
            .to_owned(),
        ));
    }
    Ok(members)
}

/// `--into x.ts`: a modul új tagjai az utolsó utasítás után (vagy `--after név` után); duplikált nevet nem fogad el.
pub(super) fn insert_module(
    doc: &mut Doc,
    program: &Program,
    rendered: &Rendered,
    style: CodeStyle,
    after: Option<&str>,
) -> GenResult<Vec<String>> {
    let members = module_members(program)?;
    let em = ScriptEmitter {
        ts: style.ts,
        semi: style.semi,
    };
    let total = doc.lines.len();
    for name in members.iter().flat_map(Member::declares) {
        if find_statement(&doc.lines, name, 0, total).is_some() {
            return Err(GenError::new(crate::tf!(
                "`{}` is already declared in the file (to change it use --replace <file>::{})",
                "a(z) `{}` már deklarálva van a fájlban (módosításhoz: --replace <fájl>::{})",
                name,
                name
            )));
        }
    }
    let unit = indent_unit(&doc.lines);
    let block = reindent(&em.module(members), unit);
    let at = match after {
        Some(name) => {
            let (_, end) = find_statement(&doc.lines, name, 0, total).ok_or_else(|| {
                GenError::new(crate::tf!(
                    "no `{}` in the file (declared: {})",
                    "nincs `{}` a fájlban (deklarált: {})",
                    name,
                    declared_names(&doc.lines, 0, total).join(", ")
                ))
            })?;
            end + 1
        }
        None => last_nonblank(&doc.lines, 0, total).map_or(0, |i| i + 1),
    };
    let spaced = at > 0 && !doc.lines[at - 1].trim().is_empty();
    let mut inserted = Vec::new();
    if spaced {
        inserted.push(String::new());
    }
    inserted.extend(block.iter().cloned());
    if after.is_some() && doc.lines.get(at).is_some_and(|l| !l.trim().is_empty()) {
        inserted.push(String::new());
    }
    doc.insert(at, inserted);
    doc.mark("members", at + usize::from(spaced), block.len());
    if !rendered.imports.is_empty() {
        let end = doc.lines.len();
        for e in plan(&doc.lines, 0, end, &rendered.imports, style.semi) {
            doc.splice(e.start, e.end, e.lines);
        }
    }
    Ok(block)
}

/// A composable tagjai: csak script-tagok, props / emits nélkül.
pub(super) fn composable_members(program: &Program) -> GenResult<&[Member]> {
    if program.template.is_some() || program.style.is_some() {
        return Err(GenError::new(
            crate::t!(
                "a .ts / .js file takes members only (no template / style): use a .vue file for those",
                "a .ts / .js fájl csak tagokat vár (template / style nélkül): azokhoz .vue fájl való"
            )
            .to_owned(),
        ));
    }
    let members = program
        .script
        .as_ref()
        .map_or(&[][..], |s| s.members.as_slice());
    if members.is_empty() {
        return Err(GenError::new(
            crate::t!(
                "the expression has no members (e.g. `comp:useCart > ref:items:list<Item>=[]`)",
                "a kifejezésben nincs tag (pl. `comp:useCart > ref:items:list<Item>=[]`)"
            )
            .to_owned(),
        ));
    }
    if members
        .iter()
        .any(|m| matches!(m, Member::Class(_) | Member::Exported { .. }))
    {
        return Err(GenError::new(
            crate::t!(
                "`class:` / `-` / `!` members belong to a module: use `mod:name>…` (or a file without composable members)",
                "a `class:` / `-` / `!` tagok modulba valók: használd a `mod:név>…` alakot (vagy composable-tag nélküli fájlt)"
            )
            .to_owned(),
        ));
    }
    if members
        .iter()
        .any(|m| matches!(m, Member::Props(_) | Member::Emits(_)))
    {
        return Err(GenError::new(
            crate::t!(
                "props() / emits() belong to a <script setup>, not to a composable",
                "a props() / emits() <script setup>-ba való, nem composable-be"
            )
            .to_owned(),
        ));
    }
    Ok(members)
}

/// `--into x.ts`: tagok a `return {` elé, a visszaadott objektum bővítése, importok.
pub(super) fn insert_script(
    doc: &mut Doc,
    program: &Program,
    rendered: &Rendered,
    style: CodeStyle,
    after: Option<&str>,
) -> GenResult<Vec<String>> {
    let members = composable_members(program)?;
    let em = ScriptEmitter {
        ts: style.ts,
        semi: style.semi,
    };
    let total = doc.lines.len();
    for name in members.iter().flat_map(Member::declares) {
        if find_statement(&doc.lines, name, 0, total).is_some() {
            return Err(GenError::new(crate::tf!(
                "`{}` is already declared in the file (to change it use --replace <file>::{})",
                "a(z) `{}` már deklarálva van a fájlban (módosításhoz: --replace <fájl>::{})",
                name,
                name
            )));
        }
    }
    let ret = find_return(&doc.lines).ok_or_else(|| {
        GenError::new(
            crate::t!(
                "no `return {` found: a composable file ends with `return { … }`",
                "nincs `return {` a fájlban: a composable-fájl `return { … }`-val végződik"
            )
            .to_owned(),
        )
    })?;
    let names: Vec<String> = members
        .iter()
        .filter_map(|m| m.declared_name().map(str::to_owned))
        .collect();
    extend_return(doc, ret, &names)?;
    let unit = indent_unit(&doc.lines);
    let block = indent_block(&reindent(&em.members(members), unit), unit);
    let at = match after {
        Some(name) => {
            let (_, end) = find_statement(&doc.lines, name, 0, ret).ok_or_else(|| {
                GenError::new(crate::tf!(
                    "no `{}` before the `return {{` (declared: {})",
                    "nincs `{}` a `return {{` előtt (deklarált: {})",
                    name,
                    declared_names(&doc.lines, 0, ret).join(", ")
                ))
            })?;
            end + 1
        }
        None => ret,
    };
    let mut inserted = Vec::new();
    let spaced = at > 0 && !doc.lines[at - 1].trim().is_empty();
    if spaced {
        inserted.push(String::new());
    }
    inserted.extend(block.iter().cloned());
    if doc.lines.get(at).is_some_and(|l| !l.trim().is_empty()) {
        inserted.push(String::new());
    }
    doc.insert(at, inserted);
    doc.mark("members", at + usize::from(spaced), block.len());
    if !rendered.imports.is_empty() {
        let end = doc.lines.len();
        for e in plan(&doc.lines, 0, end, &rendered.imports, style.semi) {
            doc.splice(e.start, e.end, e.lines);
        }
    }
    Ok(block)
}

/// A composable `return {` sora: a legkisebb behúzású utolsó ilyen sor.
pub(super) fn find_return(lines: &[String]) -> Option<usize> {
    let candidates: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].trim_start().starts_with("return {"))
        .collect();
    let min = candidates.iter().map(|&i| indent_of(&lines[i])).min()?;
    candidates
        .into_iter()
        .rfind(|&i| indent_of(&lines[i]) == min)
}

/// A visszaadott objektum bővítése az új nevekkel (egy- és többsoros alak).
pub(super) fn extend_return(doc: &mut Doc, ret: usize, names: &[String]) -> GenResult<()> {
    let line = doc.lines[ret].clone();
    let open = line.find('{').unwrap_or(0);
    if let Some(rel_close) = line[open..].rfind('}').filter(|&r| r > 0) {
        let inner = &line[open + 1..open + rel_close];
        let existing: Vec<&str> = inner.split(',').map(str::trim).collect();
        let add: Vec<&String> = names
            .iter()
            .filter(|n| !existing.contains(&n.as_str()))
            .collect();
        if add.is_empty() {
            return Ok(());
        }
        let added = add
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let inner = inner.trim().trim_end_matches(',').trim();
        let merged = if inner.is_empty() {
            added
        } else {
            format!("{inner}, {added}")
        };
        doc.lines[ret] = format!("{}{{ {merged} {}", &line[..open], &line[open + rel_close..]);
        return Ok(());
    }
    let close = (ret + 1..doc.lines.len())
        .find(|&j| doc.lines[j].trim_start().starts_with('}'))
        .ok_or_else(|| {
            GenError::new(
                crate::t!(
                    "the `return {` object is not closed",
                    "a `return {` objektum nincs lezárva"
                )
                .to_owned(),
            )
        })?;
    let existing: Vec<String> = doc.lines[ret + 1..close]
        .iter()
        .map(|l| {
            let t = l.trim().trim_end_matches(',');
            t.split(':').next().unwrap_or(t).trim().to_owned()
        })
        .collect();
    let add: Vec<&String> = names.iter().filter(|n| !existing.contains(n)).collect();
    if add.is_empty() {
        return Ok(());
    }
    let pad = " ".repeat(if close > ret + 1 {
        indent_of(&doc.lines[ret + 1])
    } else {
        indent_of(&doc.lines[ret]) + 2
    });
    if close > ret + 1 && !doc.lines[close - 1].trim_end().ends_with(',') {
        doc.lines[close - 1].push(',');
    }
    let new: Vec<String> = add.iter().map(|n| format!("{pad}{n},")).collect();
    doc.insert(close, new);
    Ok(())
}
