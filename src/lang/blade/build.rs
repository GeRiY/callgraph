//! A Blade-sablon gráfja: a szkennelt elemekből (`scan`) a `view` container, a szakaszok (`@section`, `@push`,
//! `@slot`, `<x-slot:név>`) metódusként sortartománnyal, a hívások (nézet-használat, komponensek, `route()`,
//! `__()`), a vezérlési ágak (`@if`, `@foreach`…) és a `@props` property-k.

use super::php::{self, Chunk};
use super::scan::{self, Attr, Item, literal, split_top};
use super::view;
use crate::core::LineIndex;
use crate::model::{
    Branch, Call, Container, ControlFlow, FileGraph, FileGraphBuilder, Method, Property,
};

/// A metódus `kind`-ja a sablon-blokkoknál (szakasz, push, slot, a felső szint): az outline `()` nélkül írja.
pub const BLOCK_KIND: &str = "blade";

/// A szakaszokon kívüli (felső szintű) sablon-rész pszeudo-metódusa.
const TEMPLATE: &str = "template";

/// Egy szakasz-szerű blokk (a metódusa).
#[derive(Debug)]
struct Block {
    name: String,
    start: u32,
    end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Section,
    Push,
    Slot,
}

#[derive(Debug)]
struct Open {
    family: Family,
    name: String,
    start: u32,
}

/// A direktíva neve kisbetűvel.
fn lower(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// A `x-slot:cím` / `x-slot name="cím"` slot neve.
fn slot_name(name: &str, attrs: &[Attr]) -> Option<String> {
    name.strip_prefix("x-slot:").map(str::to_owned).or_else(|| {
        attrs
            .iter()
            .find(|a| a.name == "name" && !a.value.is_empty())
            .map(|a| a.value.clone())
    })
}

/// A szakasz-szerű blokkok és sortartományaik.
fn blocks(items: &[Item], lines: &LineIndex, total: u32) -> Vec<Block> {
    let mut stack: Vec<Open> = Vec::new();
    let mut out = Vec::new();
    let close = |stack: &mut Vec<Open>, out: &mut Vec<Block>, family: Family, end: u32| {
        if let Some(at) = stack.iter().rposition(|open| open.family == family) {
            let open = stack.remove(at);
            stack.truncate(at);
            out.push(Block {
                name: open.name,
                start: open.start,
                end: end.max(open.start),
            });
        }
    };
    for item in items {
        match item {
            Item::Directive {
                name, args, start, ..
            } => {
                let line = lines.line_of(*start);
                let parts = args
                    .as_deref()
                    .map(|a| split_top(a, b','))
                    .unwrap_or_default();
                let literal_at = |index: usize| parts.get(index).and_then(|(p, _)| literal(p));
                match lower(name).as_str() {
                    "section" if parts.len() == 1 => {
                        if let Some(label) = literal_at(0) {
                            stack.push(Open {
                                family: Family::Section,
                                name: format!("section:{label}"),
                                start: line,
                            });
                        }
                    }
                    "endsection" | "show" | "stop" | "append" | "overwrite" => {
                        close(&mut stack, &mut out, Family::Section, line);
                    }
                    "push" | "pushonce" => {
                        if let Some(label) = literal_at(0) {
                            stack.push(Open {
                                family: Family::Push,
                                name: format!("push:{label}"),
                                start: line,
                            });
                        }
                    }
                    "prepend" => {
                        if let Some(label) = literal_at(0) {
                            stack.push(Open {
                                family: Family::Push,
                                name: format!("prepend:{label}"),
                                start: line,
                            });
                        }
                    }
                    "pushif" => {
                        if let Some(label) = literal_at(1) {
                            stack.push(Open {
                                family: Family::Push,
                                name: format!("push:{label}"),
                                start: line,
                            });
                        }
                    }
                    "endpush" | "endpushonce" | "endpushif" | "endprepend" => {
                        close(&mut stack, &mut out, Family::Push, line);
                    }
                    "slot" => {
                        if let Some(label) = literal_at(0) {
                            stack.push(Open {
                                family: Family::Slot,
                                name: format!("slot:{label}"),
                                start: line,
                            });
                        }
                    }
                    "endslot" => close(&mut stack, &mut out, Family::Slot, line),
                    _ => {}
                }
            }
            Item::Tag {
                name,
                start,
                close: is_close,
                self_closing,
                attrs,
            } if name.starts_with("x-slot") => {
                let line = lines.line_of(*start);
                if *is_close {
                    close(&mut stack, &mut out, Family::Slot, line);
                } else if !self_closing && let Some(label) = slot_name(name, attrs) {
                    stack.push(Open {
                        family: Family::Slot,
                        name: format!("slot:{label}"),
                        start: line,
                    });
                }
            }
            _ => {}
        }
    }
    // a fájl végéig nyitva maradt blokkok
    for open in stack {
        out.push(Block {
            name: open.name,
            start: open.start,
            end: total.max(open.start),
        });
    }
    out.sort_by_key(|b| b.start);
    out
}

/// A sor birtokosa: a legbelső tartalmazó blokk, különben a felső szintű sablon.
fn owner(blocks: &[Block], line: u32) -> String {
    blocks
        .iter()
        .filter(|b| b.start <= line && line <= b.end)
        .max_by_key(|b| (b.start, std::cmp::Reverse(b.end)))
        .map_or_else(|| TEMPLATE.to_owned(), |b| b.name.clone())
}

/// A feltételes / ciklus direktíva: a bezárója és hogy ciklus-e.
fn flow_closer(opener: &str) -> Option<(&'static str, &'static str)> {
    Some(match opener {
        "if" => ("endif", "if"),
        "unless" => ("endunless", "if"),
        "isset" => ("endisset", "if"),
        "empty" => ("endempty", "if"),
        "auth" => ("endauth", "if"),
        "guest" => ("endguest", "if"),
        "can" => ("endcan", "if"),
        "cannot" => ("endcannot", "if"),
        "canany" => ("endcanany", "if"),
        "env" => ("endenv", "if"),
        "production" => ("endproduction", "if"),
        "hassection" | "sectionmissing" => ("endif", "if"),
        "foreach" => ("endforeach", "foreach"),
        "forelse" => ("endforelse", "foreach"),
        "for" => ("endfor", "for"),
        "while" => ("endwhile", "while"),
        "switch" => ("endswitch", "switch"),
        _ => return None,
    })
}

/// Egy nyitott vezérlési szerkezet.
struct Flow {
    opener: String,
    closer: &'static str,
    kind: &'static str,
    line: u32,
    cond: String,
    owner: String,
    /// (címke, kezdősor)
    branches: Vec<(String, u32)>,
}

impl Flow {
    /// A kész szerkezet; az ágak a következő ág előtti sorig tartanak, az utolsó a záró direktíva előtti sorig.
    fn finish(self, container: &str, end: u32) -> Option<ControlFlow> {
        if self.branches.is_empty() {
            return None;
        }
        let starts: Vec<u32> = self.branches.iter().map(|(_, start)| *start).collect();
        let branches = self
            .branches
            .iter()
            .enumerate()
            .map(|(i, (label, start))| {
                let next = starts.get(i + 1).copied().unwrap_or(end);
                Branch {
                    label: label.clone(),
                    start_line: *start,
                    end_line: next.saturating_sub(1).max(*start),
                    fall_through: None,
                }
            })
            .collect();
        Some(ControlFlow {
            container: Some(container.to_owned()),
            caller: Some(self.owner),
            kind: self.kind.to_owned(),
            line: self.line,
            cond: Some(self.cond),
            branches,
        })
    }
}

/// A `foreach` fejléc iterálandó kifejezése (az ` as ` előtt, zárójelen / idézőjelen kívül).
fn iterable(args: &str) -> Option<&str> {
    let b = args.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else if c == b'\'' || c == b'"' {
            quote = Some(c);
        } else if matches!(c, b'(' | b'[' | b'{') {
            depth += 1;
        } else if matches!(c, b')' | b']' | b'}') {
            depth -= 1;
        } else if depth == 0 && b[i..].starts_with(b" as ") {
            return Some(args[..i].trim());
        }
        i += 1;
    }
    None
}

/// A `kulcs => érték` elem kulcsa és (ha van) alapértéke, felső szinten szétválasztva.
fn key_default(item: &str) -> (String, Option<String>) {
    let b = item.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else if c == b'\'' || c == b'"' {
            quote = Some(c);
        } else if matches!(c, b'(' | b'[' | b'{') {
            depth += 1;
        } else if matches!(c, b')' | b']' | b'}') {
            depth -= 1;
        } else if depth == 0 && b[i..].starts_with(b"=>") {
            return (
                item[..i].trim().to_owned(),
                Some(item[i + 2..].trim().to_owned()),
            );
        }
        i += 1;
    }
    (item.trim().to_owned(), None)
}

/// A kitett hívás-sablon (a mezők többsége üres).
fn call(container: &str, owner: &str, line: u32, kind: &str, target: &str, category: &str) -> Call {
    Call::new(kind, target, category)
        .at(line)
        .within(Some(container.to_owned()), Some(owner.to_owned()))
}

/// A PHP-elemző `route()` / `__()` hívásainak átminősítése: a route-név és a fordítási kulcs `resolved`-be kerül
/// (`route::users.store`, `i18n::auth.failed`) — ezeket a Laravel route-tábla és az i18n-kereszthivatkozás olvassa.
fn tag_helper(call: &mut Call) {
    let target = call.target.trim_start_matches('\\').to_ascii_lowercase();
    let (route, i18n) = match call.kind.as_str() {
        "func_call" => (
            target == "route",
            matches!(target.as_str(), "__" | "trans" | "trans_choice" | "lang"),
        ),
        "static_call" => (
            false,
            target.ends_with("lang::get") || target.ends_with("lang::choice"),
        ),
        _ => (false, false),
    };
    let Some(name) = php::literal_arg(call, 0) else {
        return;
    };
    if route {
        call.category = "route_helper".into();
        call.resolved = Some(format!("route::{name}"));
    } else if i18n {
        call.category = "i18n_helper".into();
        call.resolved = Some(format!("i18n::{name}"));
    }
}

/// A sablon gráfja.
pub(super) fn build(path: &str, text: &str) -> FileGraph {
    let lines = LineIndex::new(text);
    let total = u32::try_from(text.lines().count())
        .unwrap_or(u32::MAX)
        .max(1);
    let items = scan::scan(text);
    let blocks = blocks(&items, &lines, total);
    let name = view::view_name(path);
    let line_of = |at: usize| lines.line_of(at.min(text.len()));

    let mut calls: Vec<Call> = Vec::new();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut properties: Vec<Property> = Vec::new();
    let mut flows: Vec<ControlFlow> = Vec::new();
    let mut open_flows: Vec<Flow> = Vec::new();
    let mut extends: Option<String> = None;

    for item in &items {
        match item {
            Item::Echo { expr, start } => chunks.push(Chunk {
                line: line_of(*start),
                code: expr.clone(),
            }),
            Item::PhpBlock { code, start } => chunks.push(Chunk {
                line: line_of(*start),
                code: code.clone(),
            }),
            Item::Tag {
                name: tag,
                start,
                close,
                attrs,
                ..
            } => {
                if *close || tag.starts_with("x-slot") {
                    continue;
                }
                let line = line_of(*start);
                let who = owner(&blocks, line);
                let mut c = call(&name, &who, line, "component", tag, "component_use");
                c.resolved = if tag.starts_with("livewire:") {
                    view::livewire_file(tag, path)
                } else {
                    view::component_file(tag, path)
                };
                calls.push(c);
                for attr in attrs.iter().filter(|a| a.bound() && !a.value.is_empty()) {
                    chunks.push(Chunk {
                        line: line_of(attr.at),
                        code: attr.value.clone(),
                    });
                }
            }
            Item::Directive {
                name: directive,
                args,
                start,
                args_at,
            } => {
                let line = line_of(*start);
                let who = owner(&blocks, line);
                let key = lower(directive);
                let parts: Vec<(String, usize)> = args
                    .as_deref()
                    .map(|a| split_top(a, b','))
                    .unwrap_or_default();
                let lit = |index: usize| parts.get(index).and_then(|(p, _)| literal(p));
                // a `index`-edik argumentum PHP-kifejezésként
                let part_chunk = |index: usize, chunks: &mut Vec<Chunk>| {
                    if let Some((code, off)) = parts.get(index) {
                        chunks.push(Chunk {
                            line: line_of(args_at + off),
                            code: code.clone(),
                        });
                    }
                };
                let view_call = |kind: &str, view_name: &str, category: &str| {
                    let mut c = call(&name, &who, line, kind, view_name, category);
                    c.resolved = view::view_file(view_name);
                    c
                };
                match key.as_str() {
                    "extends" => {
                        if let Some(v) = lit(0) {
                            extends = Some(view::normalize(&v));
                            calls.push(view_call("extends", &v, "view_use"));
                        }
                    }
                    "include" | "includeif" => {
                        if let Some(v) = lit(0) {
                            calls.push(view_call("include", &v, "view_use"));
                        }
                        part_chunk(1, &mut chunks);
                    }
                    "includewhen" | "includeunless" => {
                        part_chunk(0, &mut chunks);
                        if let Some(v) = lit(1) {
                            calls.push(view_call("include", &v, "view_use"));
                        }
                        part_chunk(2, &mut chunks);
                    }
                    "includefirst" => {
                        if let Some((list, _)) = parts.first() {
                            let inner = list.trim().trim_start_matches('[').trim_end_matches(']');
                            for (v, _) in split_top(inner, b',') {
                                if let Some(v) = literal(&v) {
                                    calls.push(view_call("include", &v, "view_use"));
                                }
                            }
                        }
                    }
                    "each" => {
                        if let Some(v) = lit(0) {
                            calls.push(view_call("include", &v, "view_use"));
                        }
                        part_chunk(1, &mut chunks);
                    }
                    "component" | "componentfirst" => {
                        if let Some(v) = lit(0) {
                            calls.push(view_call("component", &v, "component_use"));
                        }
                        part_chunk(1, &mut chunks);
                    }
                    "yield" => {
                        if let Some(v) = lit(0) {
                            calls.push(call(
                                &name,
                                &who,
                                line,
                                "yield",
                                &format!("section:{v}"),
                                "view_use",
                            ));
                        }
                    }
                    "stack" => {
                        if let Some(v) = lit(0) {
                            calls.push(call(
                                &name,
                                &who,
                                line,
                                "stack",
                                &format!("stack:{v}"),
                                "view_use",
                            ));
                        }
                    }
                    "lang" | "choice" => {
                        if let Some(v) = lit(0) {
                            let mut c = call(&name, &who, line, "directive", "lang", "i18n_helper");
                            c.resolved = Some(format!("i18n::{v}"));
                            calls.push(c);
                        }
                    }
                    "csrf" => calls.push(call(
                        &name,
                        &who,
                        line,
                        "directive",
                        "@csrf",
                        "framework_use",
                    )),
                    "method" => {
                        let verb = lit(0).unwrap_or_default();
                        calls.push(call(
                            &name,
                            &who,
                            line,
                            "directive",
                            &format!("@method({verb})"),
                            "framework_use",
                        ));
                    }
                    "vite" => calls.push(call(
                        &name,
                        &who,
                        line,
                        "directive",
                        "@vite",
                        "framework_use",
                    )),
                    "props" => {
                        if let Some(list) = args.as_deref() {
                            let lead = list.len() - list.trim_start().len();
                            let trimmed = list.trim();
                            let (inner, shift) = if let Some(rest) =
                                trimmed.strip_prefix('[').and_then(|r| r.strip_suffix(']'))
                            {
                                (rest, 1)
                            } else if let Some(rest) = trimmed
                                .strip_prefix("array(")
                                .and_then(|r| r.strip_suffix(')'))
                            {
                                (rest, "array(".len())
                            } else {
                                ("", 0)
                            };
                            for (item, off) in split_top(inner, b',') {
                                let (key, default) = key_default(&item);
                                let Some(prop) = literal(&key) else { continue };
                                properties.push(Property {
                                    container: Some(name.clone()),
                                    name: prop,
                                    line: line_of(args_at + lead + shift + off),
                                    type_hint: None,
                                    visibility: "public".into(),
                                    is_static: false,
                                    readonly: false,
                                    doc: None,
                                    default,
                                    js: false,
                                });
                            }
                        }
                    }
                    "php" => part_chunk(0, &mut chunks),
                    "class" | "style" | "json" | "js" | "checked" | "selected" | "disabled"
                    | "readonly" | "required" | "dump" | "dd" => {
                        if let Some(raw) = args.as_deref() {
                            let lead = raw.len() - raw.trim_start().len();
                            chunks.push(Chunk {
                                line: line_of(args_at + lead),
                                code: raw.trim().to_owned(),
                            });
                        }
                    }
                    _ => {}
                }
                flow_directive(
                    &key,
                    args.as_deref(),
                    *args_at,
                    line,
                    &who,
                    &name,
                    &mut open_flows,
                    &mut flows,
                    &mut chunks,
                    &line_of,
                );
            }
        }
    }
    // a fájl végéig nyitva maradt szerkezetek
    for flow in open_flows {
        flows.extend(flow.finish(&name, total + 1));
    }

    for mut c in php::calls(&chunks) {
        c.container = Some(name.clone());
        c.caller = Some(owner(&blocks, c.line));
        tag_helper(&mut c);
        calls.push(c);
    }
    calls.sort_by_key(|c| c.line);

    let mut b = FileGraphBuilder::new(path.to_owned());
    b.container(Container {
        kind: "view".into(),
        name: name.clone(),
        line: 1,
        end_line: total,
        is_abstract: Some(false),
        is_final: Some(false),
        extends: Some(extends.into_iter().collect()),
        implements: Some(vec![]),
        traits: Some(vec![]),
        doc: None,
    });
    let method = |label: &str, start: u32, end: u32| Method {
        container: Some(name.clone()),
        name: label.to_owned(),
        line: start,
        end_line: Some(end),
        visibility: None,
        is_static: false,
        is_abstract: false,
        params: vec![],
        return_type: None,
        doc_types: None,
        doc: None,
        body: None,
        kind: Some(BLOCK_KIND.to_owned()),
    };
    let top_used = calls.iter().any(|c| c.caller.as_deref() == Some(TEMPLATE))
        || flows.iter().any(|f| f.caller.as_deref() == Some(TEMPLATE));
    if top_used {
        b.method(method(TEMPLATE, 1, total));
    }
    for block in &blocks {
        b.method(method(&block.name, block.start, block.end));
    }
    for prop in properties {
        b.property(prop);
    }
    for c in calls {
        b.call(c);
    }
    for flow in flows {
        b.control_flow(flow);
    }
    b.build()
}

/// Egy direktíva hatása a vezérlési szerkezetekre (megnyitás, ág, lezárás), és a feltételei PHP-kifejezésként.
#[allow(clippy::too_many_arguments)]
fn flow_directive(
    key: &str,
    args: Option<&str>,
    args_at: usize,
    line: u32,
    who: &str,
    container: &str,
    open: &mut Vec<Flow>,
    done: &mut Vec<ControlFlow>,
    chunks: &mut Vec<Chunk>,
    line_of: &dyn Fn(usize) -> u32,
) {
    let arg_chunk = |chunks: &mut Vec<Chunk>, code: &str, offset: usize| {
        chunks.push(Chunk {
            line: line_of(args_at + offset),
            code: code.to_owned(),
        });
    };
    let trimmed = args.map(str::trim);
    let lead = args.map_or(0, |a| a.len() - a.trim_start().len());
    // a `@forelse` ága: argumentum nélküli `@empty`
    if key == "empty"
        && args.is_none()
        && let Some(top) = open.last_mut().filter(|f| f.opener == "forelse")
    {
        top.branches.push(("empty".to_owned(), line));
        return;
    }
    if let Some((closer, kind)) = flow_closer(key)
        && (args.is_some() || matches!(key, "auth" | "guest" | "production" | "env"))
    {
        let has_args = args.is_some();
        // a `@if (...)`-nek kötelező az argumentuma; a `@auth` / `@guest` / `@production` nélküle is nyit
        if !has_args && !matches!(key, "auth" | "guest" | "production") {
            return;
        }
        let shown = trimmed.unwrap_or_default();
        let (cond, label) = match key {
            "if" => (shown.to_owned(), format!("if ({shown})")),
            "unless" => (format!("!({shown})"), format!("if (!({shown}))")),
            "foreach" | "forelse" | "for" | "while" | "switch" => {
                (shown.to_owned(), format!("{kind} ({shown})"))
            }
            "auth" | "guest" | "production" => {
                let cond = if shown.is_empty() {
                    key.to_owned()
                } else {
                    format!("{key}({shown})")
                };
                let label = format!("if ({cond})");
                (cond, label)
            }
            "hassection" => (
                format!("hasSection({shown})"),
                format!("if (hasSection({shown}))"),
            ),
            "sectionmissing" => (
                format!("sectionMissing({shown})"),
                format!("if (sectionMissing({shown}))"),
            ),
            other => {
                let cond = format!("{other}({shown})");
                let label = format!("if ({cond})");
                (cond, label)
            }
        };
        let branches = if key == "switch" {
            Vec::new()
        } else {
            vec![(label, line)]
        };
        open.push(Flow {
            opener: key.to_owned(),
            closer,
            kind,
            line,
            cond,
            owner: who.to_owned(),
            branches,
        });
        // a feltétel hívásai
        match key {
            "if" | "unless" | "while" | "switch" => {
                if let Some(code) = trimmed {
                    arg_chunk(&mut *chunks, code, lead);
                }
            }
            "foreach" | "forelse" => {
                if let Some(code) = args.and_then(iterable) {
                    let offset = args.map_or(0, |a| a.find(code).unwrap_or(0));
                    arg_chunk(&mut *chunks, code, offset);
                }
            }
            "for" => {
                if let Some(code) = trimmed {
                    arg_chunk(&mut *chunks, &format!("for ({code}) {{}}"), lead);
                }
            }
            _ => {}
        }
        return;
    }
    match key {
        "elseif" => {
            if let (Some(top), Some(code)) = (open.last_mut().filter(|f| f.kind == "if"), trimmed) {
                top.branches.push((format!("elseif ({code})"), line));
                arg_chunk(&mut *chunks, code, lead);
            }
        }
        "elsecan" | "elsecannot" => {
            if let Some(top) = open.last_mut().filter(|f| f.kind == "if") {
                let shown = trimmed.unwrap_or_default();
                top.branches
                    .push((format!("elseif ({}({shown}))", &key[4..]), line));
            }
        }
        "else" => {
            if let Some(top) = open.last_mut().filter(|f| f.kind == "if") {
                top.branches.push(("else".to_owned(), line));
            }
        }
        "case" => {
            if let (Some(top), Some(code)) =
                (open.last_mut().filter(|f| f.kind == "switch"), trimmed)
            {
                top.branches.push((format!("case {code}"), line));
            }
        }
        "default" => {
            if let Some(top) = open.last_mut().filter(|f| f.kind == "switch") {
                top.branches.push(("default".to_owned(), line));
            }
        }
        _ => {
            if let Some(at) = open
                .iter()
                .rposition(|f| f.closer.eq_ignore_ascii_case(key))
            {
                let flow = open.remove(at);
                // a fölötte nyitva maradt szerkezetek ezen a soron zárulnak
                let above: Vec<Flow> = open.drain(at..).collect();
                for flow in above {
                    done.extend(flow.finish(container, line));
                }
                done.extend(flow.finish(container, line));
            }
        }
    }
}
