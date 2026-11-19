//! The template facts the Vue component contract views read (`--format props`): what each component tag passes
//! (attributes, `v-model`s, slots), and what the template itself offers (`<slot>`s) and uses (`$attrs`).

use super::node::{Element, Node, Prop};

/// One component tag in a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentUse {
    /// The tag as written (`MessageActions`, `message-actions`).
    pub tag: String,
    pub line: u32,
    /// Static attribute names and `:name` / `v-bind:name` arguments as written (no `class` / listener filtering).
    pub attrs: Vec<String>,
    /// `v-bind="obj"` or a dynamic argument: the full prop set is not statically known.
    pub dynamic: bool,
    /// `v-model` (`modelValue`) and `v-model:name` arguments.
    pub models: Vec<String>,
    /// Slots the parent fills: `<template #x>` / `v-slot:x` (`default` for the unnamed ones).
    pub slots: Vec<(String, u32)>,
    /// A slot name is computed (`#[name]`): the slot set is not statically known.
    pub slots_dynamic: bool,
}

/// The contract-relevant facts of one template.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateFacts {
    pub uses: Vec<ComponentUse>,
    /// The `<slot>` outlets (`default` for the unnamed one) with their lines.
    pub slots: Vec<(String, u32)>,
    /// A `<slot :name="…">` outlet exists: the slot set is not statically known.
    pub slots_dynamic: bool,
    /// The template reads `$attrs`.
    pub attrs_used: bool,
}

/// Built-in / structural tags that are never user components.
const BUILTIN: [&str; 9] = [
    "component",
    "slot",
    "template",
    "transition",
    "transition-group",
    "keep-alive",
    "teleport",
    "suspense",
    "style",
];

pub(super) fn collect(nodes: &[Node]) -> TemplateFacts {
    let mut facts = TemplateFacts::default();
    walk(nodes, &mut facts);
    facts
}

fn walk(nodes: &[Node], facts: &mut TemplateFacts) {
    for node in nodes {
        match node {
            Node::Element(el) => element(el, facts),
            Node::Interpolation { content, .. } => {
                facts.attrs_used |= content.contains("$attrs");
            }
            Node::Text { .. } => {}
        }
    }
}

fn element(el: &Element, facts: &mut TemplateFacts) {
    for prop in &el.props {
        if let Prop::Dir { exp: Some(exp), .. } = prop {
            facts.attrs_used |= exp.contains("$attrs");
        }
    }
    if el.tag == "slot" {
        outlet(el, facts);
    } else if is_component_tag(&el.tag) {
        facts.uses.push(component(el));
    }
    walk(&el.children, facts);
}

fn is_component_tag(tag: &str) -> bool {
    if BUILTIN.contains(&tag.to_ascii_lowercase().as_str()) {
        return false;
    }
    tag.starts_with(|c: char| c.is_ascii_uppercase()) || tag.contains('-')
}

/// `<slot>` / `<slot name="x">` / `<slot :name="x">`.
fn outlet(el: &Element, facts: &mut TemplateFacts) {
    let mut name = None;
    let mut dynamic = false;
    for prop in &el.props {
        match prop {
            Prop::Attr {
                name: attr,
                value: Some(v),
                ..
            } if attr == "name" => name = Some(v.clone()),
            Prop::Dir {
                name: dir,
                arg: Some(arg),
                ..
            } if dir == "bind" && arg == "name" => dynamic = true,
            _ => {}
        }
    }
    if dynamic {
        facts.slots_dynamic = true;
    } else {
        facts
            .slots
            .push((name.unwrap_or_else(|| "default".to_string()), el.line));
    }
}

fn component(el: &Element) -> ComponentUse {
    let mut out = ComponentUse {
        tag: el.tag.clone(),
        line: el.line,
        attrs: Vec::new(),
        dynamic: false,
        models: Vec::new(),
        slots: Vec::new(),
        slots_dynamic: false,
    };
    for prop in &el.props {
        match prop {
            Prop::Attr { name, .. } => out.attrs.push(name.clone()),
            Prop::Dir { name, arg, .. } => match (name.as_str(), arg.as_deref()) {
                ("bind", Some(a)) if !a.starts_with('[') => out.attrs.push(a.to_string()),
                ("bind", _) => out.dynamic = true,
                ("model", None) => out.models.push("modelValue".to_string()),
                ("model", Some(a)) if !a.starts_with('[') => out.models.push(a.to_string()),
                ("model", Some(_)) => out.dynamic = true,
                ("slot", arg) => slot_name(arg, el.line, &mut out),
                _ => {}
            },
        }
    }
    for child in &el.children {
        let Node::Element(inner) = child else {
            continue;
        };
        if inner.tag != "template" {
            continue;
        }
        for prop in &inner.props {
            if let Prop::Dir {
                name, arg, line, ..
            } = prop
                && name == "slot"
            {
                slot_name(arg.as_deref(), *line, &mut out);
            }
        }
    }
    out
}

fn slot_name(arg: Option<&str>, line: u32, out: &mut ComponentUse) {
    match arg {
        Some(a) if a.starts_with('[') => out.slots_dynamic = true,
        Some(a) if !a.is_empty() => out.slots.push((a.to_string(), line)),
        _ => out.slots.push(("default".to_string(), line)),
    }
}
