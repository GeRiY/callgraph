//! The Vue component contract (`--format props`): what a parent passes to a child component against what the child
//! declares.
//!
//! - missing required props (`required: true`, or a non-optional member of `defineProps<{ … }>()` without a
//!   `withDefaults` value) and unknown props (an attribute the child's statically known prop set does not hold);
//! - parent slots (`<template #x>`, `v-slot:x`) the child never offers (`<slot name="x">`);
//! - `v-model` / `v-model:x` count as the prop (and its `update:` event), kebab-case and camelCase names are the
//!   same name.
//!
//! The child is found through the parent's `.vue` import (or, when the project has `unplugin-vue-components`, by an
//! exact PascalCase file name). Everything is conservative: no finding without a statically known prop set (no
//! `$attrs` / `useAttrs` / `inheritAttrs` / `mixins` / `extends`, no imported or composed props type), no
//! "missing" when the parent spreads `v-bind="obj"`, and standard HTML attributes fall through to the child's root
//! element, so they are never "unknown".

mod render;
mod script;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;

use crate::analysis::Analyzer;
use crate::core::diag::Diagnostics;
use crate::io::FileWalker;
use crate::lang::js::{Sfc, TemplateFacts, template_facts};
use crate::query::QueryResult;

/// Files bigger than this are not read.
const MAX_BYTES: u64 = 1_500_000;

/// One declared prop (camelCase).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Prop {
    name: String,
    required: bool,
}

/// What the analysis knows about one `.vue` file.
#[derive(Debug)]
struct Comp {
    rel: String,
    stem: String,
    props: Vec<Prop>,
    /// `defineModel` names (camelCase).
    models: Vec<String>,
    /// The declared props are all visible: an unknown attribute is a finding.
    props_complete: bool,
    /// The `<slot>` outlets (`None`: not statically known).
    slots: Option<Vec<String>>,
    facts: TemplateFacts,
    /// `alias -> displayed import path` of the script.
    imports: Vec<(String, String)>,
}

/// The finding kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    MissingProp,
    UnknownProp,
    UnknownSlot,
}

/// One finding at a component tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) kind: Kind,
    pub(crate) name: String,
    /// The tag as written.
    pub(crate) tag: String,
    /// The child was bound by `unplugin-vue-components` auto-import (no import in the file).
    pub(crate) auto: bool,
}

/// The findings of the queried files.
#[derive(Debug, Default)]
pub(crate) struct VueContract {
    /// Component uses of the queried files that were bound to a `.vue` child.
    checked: usize,
    findings: Vec<Finding>,
    /// The uses that could not be checked (footer of the view).
    diagnostics: Diagnostics,
}

impl VueContract {
    /// Scan the project below `root`; only the uses inside the `scope` files are checked.
    pub(crate) fn open(root: &Path, base: &Path, scope: &HashSet<String>) -> Self {
        let walker = FileWalker::for_extensions(vec!["vue", "json", "ts"]);
        let files: Vec<(String, PathBuf)> = walker
            .collect(&[root.to_path_buf()], &mut |_| {})
            .into_iter()
            .filter_map(|path| Some((relative(&path, root, base)?, path)))
            .collect();
        let auto = auto_import_enabled(&files);
        let analyzer = Analyzer::with_defaults();
        let comps: Vec<Comp> = files
            .par_iter()
            .filter(|(rel, _)| rel.ends_with(".vue"))
            .filter_map(|(rel, path)| read_comp(&analyzer, rel, path))
            .collect();
        let project = Project { comps, auto };
        project.check(scope)
    }
}

/// The scanned `.vue` files.
struct Project {
    comps: Vec<Comp>,
    auto: bool,
}

impl Project {
    fn check(&self, scope: &HashSet<String>) -> VueContract {
        let mut out = VueContract::default();
        for comp in self.comps.iter().filter(|c| scope.contains(&c.rel)) {
            for usage in &comp.facts.uses {
                let Some((child, auto)) = self.bind(comp, &usage.tag) else {
                    let pas = pascal(&usage.tag);
                    let imported_vue = comp.imports.iter().any(|(alias, display)| {
                        (alias == &usage.tag || *alias == pas) && display.ends_with(".vue")
                    });
                    if imported_vue {
                        out.diagnostics.push(
                            "props-child",
                            (
                                "child component file not found or ambiguous",
                                "a gyerek-komponens fájlja nem található vagy nem egyértelmű",
                            ),
                            &comp.rel,
                            usage.line,
                        );
                    }
                    continue;
                };
                out.checked += 1;
                let child = &self.comps[child];
                let mut push = |kind: Kind, name: &str| {
                    out.findings.push(Finding {
                        file: comp.rel.clone(),
                        line: usage.line,
                        kind,
                        name: name.to_string(),
                        tag: usage.tag.clone(),
                        auto,
                    });
                };
                let passed: HashSet<String> = usage
                    .attrs
                    .iter()
                    .chain(&usage.models)
                    .map(|name| camelize(name))
                    .collect();
                if usage.dynamic {
                    out.diagnostics.push(
                        "props-spread",
                        (
                            "v-bind object: required props not checked",
                            "v-bind objektum: a kötelező props nincs ellenőrizve",
                        ),
                        &comp.rel,
                        usage.line,
                    );
                } else {
                    for prop in child.props.iter().filter(|p| p.required) {
                        if !passed.contains(&prop.name) {
                            push(Kind::MissingProp, &prop.name);
                        }
                    }
                }
                if child.props_complete {
                    for attr in &usage.attrs {
                        let name = camelize(attr);
                        let declared = child.props.iter().any(|p| p.name == name)
                            || child.models.contains(&name);
                        if !declared && !falls_through(attr) {
                            push(Kind::UnknownProp, attr);
                        }
                    }
                }
                if let Some(slots) = &child.slots {
                    for (name, _) in usage.slots.iter().filter(|(n, _)| n != "default") {
                        if !slots.contains(name) {
                            push(Kind::UnknownSlot, name);
                        }
                    }
                }
            }
        }
        out
    }

    /// The child component of a tag: through the file's `.vue` import, else (with the auto-import plugin) the one
    /// file named like the tag. The flag is `true` for the auto-import.
    fn bind(&self, from: &Comp, tag: &str) -> Option<(usize, bool)> {
        let pas = pascal(tag);
        let import = from
            .imports
            .iter()
            .find(|(alias, _)| alias == tag)
            .or_else(|| from.imports.iter().find(|(alias, _)| *alias == pas));
        if let Some((_, display)) = import {
            return self.by_display(display).map(|at| (at, false));
        }
        if !self.auto {
            return None;
        }
        let mut named = self
            .comps
            .iter()
            .enumerate()
            .filter(|(_, comp)| comp.stem == pas);
        match (named.next(), named.next()) {
            (Some((at, _)), None) => Some((at, true)),
            _ => None,
        }
    }

    /// The one `.vue` file an import display (`src/components/Card.vue`, `@kit/Tag.vue`) names; ambiguous or
    /// missing: `None`.
    fn by_display(&self, display: &str) -> Option<usize> {
        if !display.ends_with(".vue") {
            return None;
        }
        let mut key = display;
        while let Some(rest) = key.strip_prefix("./").or_else(|| key.strip_prefix("../")) {
            key = rest;
        }
        // an alias (`@kit/…`, `@/…`, `~/…`): the first segment is not part of the path
        if key.starts_with(['@', '~'])
            && let Some((_, rest)) = key.split_once('/')
        {
            key = rest;
        }
        let tail = format!("/{key}");
        let mut hits = self
            .comps
            .iter()
            .enumerate()
            .filter(|(_, comp)| comp.rel == key || comp.rel.ends_with(&tail));
        match (hits.next(), hits.next()) {
            (Some((at, _)), None) => Some(at),
            _ => None,
        }
    }
}

fn read_comp(analyzer: &Analyzer, rel: &str, path: &Path) -> Option<Comp> {
    let text = read(path)?;
    let sfc = Sfc::parse(&text);
    let graph = analyzer.analyze_path(path).ok()?.graph;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let script = script::script_text(&text, &sfc);
    let props = graph
        .properties
        .iter()
        .filter(|p| p.js && p.container.as_deref() == Some(stem.as_str()))
        .map(|p| Prop {
            name: camelize(&p.name),
            required: p.readonly,
        })
        .collect();
    let models = graph
        .constants
        .iter()
        .filter_map(|c| c.name.strip_prefix("model:"))
        .map(camelize)
        .collect();
    let facts = template_facts(&text, &sfc);
    let opaque = [
        "$slots",
        "useSlots",
        "defineSlots",
        "renderSlot",
        "render(",
        "render:",
    ]
    .iter()
    .any(|needle| text.contains(needle))
        || script::has_jsx(&sfc);
    let slots = facts
        .as_ref()
        .filter(|f| !f.slots_dynamic && !opaque)
        .map(|f| f.slots.iter().map(|(name, _)| name.clone()).collect());
    let attrs_used = facts.as_ref().is_some_and(|f| f.attrs_used);
    let facts = facts.unwrap_or_default();
    Some(Comp {
        rel: rel.to_string(),
        stem,
        props,
        models,
        props_complete: !attrs_used && script::props_complete(&script),
        slots,
        facts,
        imports: graph.imports.parts().0.to_vec(),
    })
}

/// `unplugin-vue-components` in a `package.json`, or a generated `components.d.ts`.
fn auto_import_enabled(files: &[(String, PathBuf)]) -> bool {
    files.iter().any(|(rel, path)| {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        name == "components.d.ts"
            || (name == "package.json"
                && read(path).is_some_and(|text| text.contains("unplugin-vue-components")))
    })
}

fn read(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > MAX_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Attributes that are meant for the child's root element, not for a prop: class / style / key / ref, `data-*` /
/// `aria-*`, listeners, and the standard HTML attributes.
fn falls_through(attr: &str) -> bool {
    const HTML: [&str; 115] = [
        "class",
        "style",
        "key",
        "ref",
        "ref_for",
        "is",
        "slot",
        "id",
        "role",
        "tabindex",
        "hidden",
        "lang",
        "dir",
        "draggable",
        "accesskey",
        "title",
        "name",
        "href",
        "src",
        "alt",
        "type",
        "value",
        "for",
        "disabled",
        "placeholder",
        "target",
        "rel",
        "width",
        "height",
        "checked",
        "readonly",
        "required",
        "maxlength",
        "minlength",
        "min",
        "max",
        "step",
        "autofocus",
        "autocomplete",
        "rows",
        "cols",
        "loading",
        "download",
        "action",
        "method",
        "selected",
        "multiple",
        "accept",
        "pattern",
        "size",
        "contenteditable",
        "enterkeyhint",
        "inputmode",
        "inert",
        "nonce",
        "part",
        "popover",
        "spellcheck",
        "translate",
        "autocapitalize",
        "itemprop",
        "itemscope",
        "itemtype",
        "itemid",
        "itemref",
        "form",
        "formaction",
        "formenctype",
        "formmethod",
        "formnovalidate",
        "formtarget",
        "list",
        "dirname",
        "capture",
        "enctype",
        "novalidate",
        "wrap",
        "label",
        "open",
        "srcset",
        "sizes",
        "media",
        "hreflang",
        "ping",
        "referrerpolicy",
        "crossorigin",
        "decoding",
        "fetchpriority",
        "usemap",
        "poster",
        "preload",
        "autoplay",
        "controls",
        "loop",
        "muted",
        "playsinline",
        "colspan",
        "rowspan",
        "headers",
        "scope",
        "datetime",
        "cite",
        "start",
        "reversed",
        "high",
        "low",
        "optimum",
        "span",
        "sandbox",
        "allow",
        "allowfullscreen",
        "frameborder",
        "srcdoc",
        "popovertarget",
        "popovertargetaction",
    ];
    let lower = attr.to_ascii_lowercase();
    HTML.contains(&lower.as_str())
        || lower.starts_with("data-")
        || lower.starts_with("aria-")
        || lower.starts_with("xlink:")
        || lower.starts_with("xmlns")
        || lower.contains(':')
        // `onClick` / `onUpdate:x`: a listener written as an attribute
        || (attr.starts_with("on") && attr[2..].starts_with(|c: char| c.is_ascii_uppercase()))
}

/// `some-prop` -> `someProp`.
fn camelize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut up = false;
    for c in name.chars() {
        if c == '-' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `message-actions` -> `MessageActions` (a PascalCase tag stays).
fn pascal(tag: &str) -> String {
    let camel = camelize(tag);
    let mut chars = camel.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The project-relative, `/`-separated path.
fn relative(path: &Path, root: &Path, base: &Path) -> Option<String> {
    let stripped = path
        .strip_prefix(base)
        .ok()
        .or_else(|| path.strip_prefix(root).ok());
    let rel = match stripped {
        Some(rel) => rel,
        None if path.is_absolute() => return None,
        None => path,
    };
    let text = rel.to_string_lossy().replace('\\', "/");
    Some(text.trim_start_matches("./").to_owned())
}

/// The rows with the contract findings, when the view asks for them (`props`); nothing is scanned unless a queried
/// row is a `.vue` file.
pub(crate) fn attach(rows: &mut [QueryResult], root: &Path, format: &str) {
    if rows.is_empty() || format != "props" {
        return;
    }
    let base = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let scope: HashSet<String> = rows
        .iter()
        .filter_map(|row| relative(Path::new(&row.graph.file), root, &base))
        .filter(|rel| rel.ends_with(".vue"))
        .collect();
    if scope.is_empty() {
        return;
    }
    let shared = Arc::new(VueContract::open(root, &base, &scope));
    for row in rows {
        row.vue_contract = Some(Arc::clone(&shared));
    }
}
