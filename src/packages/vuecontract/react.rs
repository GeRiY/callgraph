//! The React side of the `props` view: the JSX uses of a function component against the props it declares (the
//! `react_comp` / `react_prop` facts of the file graph, see `lang::js::react`).
//!
//! Conservative: a `{...spread}` attribute suppresses "missing", an open prop set (a `...rest`, an untyped `props`
//! object, `HTMLAttributes` / `Omit<…>` / unresolved types, a component without parameters) suppresses "unknown",
//! and `key` / `ref` / `children` / `className` / `style` / `data-*` / `aria-*` are never "unknown". Only a
//! component that is bound statically (a local function component, or an import of a project file whose export is
//! the component itself; a HOC-wrapped export is not) is checked.

use std::collections::HashSet;
use std::path::Path;

use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};

use super::{Finding, Kind, Prop, VueContract, read};
use crate::analysis::Analyzer;
use crate::lang::js::react::{COMP_KIND, FLAG_DEFAULT, FLAG_EXPORT, FLAG_OPEN, PROP_KIND};
use crate::lang::js::{LineIndex, parse_program, source_type_for};

/// One declared React component.
#[derive(Debug)]
pub(super) struct ReactComp {
    name: String,
    default: bool,
    exports: Vec<String>,
    open: bool,
    props: Vec<Prop>,
}

/// One `<Tag …>` use.
#[derive(Debug)]
struct Usage {
    tag: String,
    line: u32,
    attrs: Vec<String>,
    spread: bool,
    children: bool,
}

/// What the analysis knows about one `.jsx` / `.tsx` / `.js` file.
#[derive(Debug)]
pub(super) struct ReactFile {
    rel: String,
    comps: Vec<ReactComp>,
    /// `alias -> displayed import path` (`path::Name` for a named import, `path` for the default one).
    imports: Vec<(String, String)>,
    /// The uses of capitalised tags; only read for the queried files.
    uses: Vec<Usage>,
}

/// Does the path hold React components.
pub(super) fn is_react_path(rel: &str) -> bool {
    rel.rsplit_once('.').is_some_and(|(_, ext)| {
        matches!(
            ext.to_ascii_lowercase().as_str(),
            "jsx" | "tsx" | "js" | "mjs"
        )
    })
}

/// The facts of one file; `None` without JSX.
pub(super) fn read_file(
    analyzer: &Analyzer,
    rel: &str,
    path: &Path,
    in_scope: bool,
) -> Option<ReactFile> {
    let text = read(path)?;
    if !text.contains("</") && !text.contains("/>") {
        return None;
    }
    let graph = analyzer.analyze_path(path).ok()?.graph;
    let mut comps = Vec::new();
    for c in graph.constants.iter().filter(|c| c.kind == COMP_KIND) {
        let flags: Vec<&str> = c.value.as_deref().unwrap_or("").split(',').collect();
        let prefix = format!("{}.", c.name);
        let props = graph
            .constants
            .iter()
            .filter(|p| p.kind == PROP_KIND)
            .filter_map(|p| {
                Some(Prop {
                    name: p.name.strip_prefix(&prefix)?.to_string(),
                    required: p.value.is_some(),
                })
            })
            .collect();
        comps.push(ReactComp {
            name: c.name.clone(),
            default: flags.contains(&FLAG_DEFAULT),
            exports: flags
                .iter()
                .filter_map(|f| f.strip_prefix(FLAG_EXPORT))
                .map(str::to_string)
                .collect(),
            open: flags.contains(&FLAG_OPEN),
            props,
        });
    }
    let uses = if in_scope {
        read_uses(path, &text)
    } else {
        Vec::new()
    };
    Some(ReactFile {
        rel: rel.to_string(),
        comps,
        imports: graph.imports.parts().0.to_vec(),
        uses,
    })
}

fn read_uses(path: &Path, text: &str) -> Vec<Usage> {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = parse_program(&allocator, text, source_type_for(path, None));
    let lines = LineIndex::new(text);
    let mut visitor = UseVisitor {
        lines: &lines,
        uses: Vec::new(),
    };
    visitor.visit_program(&parsed.program);
    visitor.uses
}

struct UseVisitor<'l> {
    lines: &'l LineIndex,
    uses: Vec<Usage>,
}

impl<'a> Visit<'a> for UseVisitor<'_> {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        let open = &it.opening_element;
        if let JSXElementName::IdentifierReference(id) = &open.name
            && id
                .name
                .as_str()
                .starts_with(|c: char| c.is_ascii_uppercase())
        {
            let mut usage = Usage {
                tag: id.name.to_string(),
                line: self.lines.line_of(open.span.start as usize),
                attrs: Vec::new(),
                spread: false,
                children: it.children.iter().any(|c| match c {
                    JSXChild::Text(t) => !t.value.as_str().trim().is_empty(),
                    JSXChild::ExpressionContainer(e) => {
                        !matches!(e.expression, JSXExpression::EmptyExpression(_))
                    }
                    _ => true,
                }),
            };
            for item in &open.attributes {
                match item {
                    JSXAttributeItem::SpreadAttribute(_) => usage.spread = true,
                    JSXAttributeItem::Attribute(a) => {
                        if let JSXAttributeName::Identifier(name) = &a.name {
                            usage.attrs.push(name.name.to_string());
                        }
                    }
                }
            }
            self.uses.push(usage);
        }
        walk::walk_jsx_element(self, it);
    }
}

/// The findings of the queried files' JSX uses.
pub(super) fn check(files: &[ReactFile], scope: &HashSet<String>, out: &mut VueContract) {
    for file in files.iter().filter(|f| scope.contains(&f.rel)) {
        for usage in &file.uses {
            let Some(comp) = bind(files, file, &usage.tag) else {
                continue;
            };
            out.checked += 1;
            let mut push = |kind: Kind, name: &str| {
                out.findings.push(Finding {
                    file: file.rel.clone(),
                    line: usage.line,
                    kind,
                    name: name.to_string(),
                    tag: usage.tag.clone(),
                    auto: false,
                });
            };
            if usage.spread {
                out.diagnostics.push(
                    "props-spread-jsx",
                    (
                        "{...spread}: required props not checked",
                        "{...spread}: a kötelező props nincs ellenőrizve",
                    ),
                    &file.rel,
                    usage.line,
                );
            } else {
                for prop in comp.props.iter().filter(|p| p.required) {
                    let given = usage.attrs.contains(&prop.name)
                        || (prop.name == "children" && usage.children);
                    if !given {
                        push(Kind::MissingProp, &prop.name);
                    }
                }
            }
            if !comp.open {
                for attr in &usage.attrs {
                    if !comp.props.iter().any(|p| &p.name == attr) && !falls_through(attr) {
                        push(Kind::UnknownProp, attr);
                    }
                }
            }
        }
    }
}

/// Attributes React itself handles, or that carry no component contract.
fn falls_through(attr: &str) -> bool {
    matches!(attr, "key" | "ref" | "children" | "className" | "style")
        || attr.starts_with("data-")
        || attr.starts_with("aria-")
}

/// The component a tag stands for: a function component of the same file, or the one export of a project file
/// the tag is imported from.
fn bind<'f>(files: &'f [ReactFile], from: &'f ReactFile, tag: &str) -> Option<&'f ReactComp> {
    if let Some(local) = from.comps.iter().find(|c| c.name == tag) {
        return Some(local);
    }
    let (_, display) = from.imports.iter().find(|(alias, _)| alias == tag)?;
    let (file_part, name) = match display.rsplit_once("::") {
        Some((_, "*")) => return None,
        Some((file, name)) => (file, Some(name)),
        None => (display.as_str(), None),
    };
    let target = by_display(files, file_part)?;
    match name {
        Some(name) => target
            .comps
            .iter()
            .find(|c| c.exports.iter().any(|e| e == name)),
        None => target.comps.iter().find(|c| c.default),
    }
}

/// The one scanned file an import display (`src/components/Card.tsx`) names; ambiguous or missing: `None`.
fn by_display<'f>(files: &'f [ReactFile], display: &str) -> Option<&'f ReactFile> {
    let mut key = display;
    while let Some(rest) = key.strip_prefix("./").or_else(|| key.strip_prefix("../")) {
        key = rest;
    }
    // an unresolved alias (`@/…`, `~/…`): the first segment is not part of the path
    if key.starts_with(['@', '~'])
        && let Some((_, rest)) = key.split_once('/')
    {
        key = rest;
    }
    let tail = format!("/{key}");
    let mut hits = files
        .iter()
        .filter(|f| f.rel == key || f.rel.ends_with(&tail));
    match (hits.next(), hits.next()) {
        (Some(file), None) => Some(file),
        _ => None,
    }
}
