//! A template-fa bejárása: `calls`, `strings`, `controlFlow` a `caller:'template'` sorokhoz.

use std::collections::HashSet;

use super::entities::decode;
use super::expr::{expr_calls, expr_strings, pascal};
use super::node::{Element, Node, Prop};
use crate::model::{Branch, Call, ControlFlow, Imports, StringLit};

const OBJECT_PROTO: &[&str] = &[
    "constructor",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
    "__proto__",
];

/// A template-elemzés kimenete.
#[derive(Default)]
pub struct TemplateOut {
    pub calls: Vec<Call>,
    pub strings: Vec<StringLit>,
    pub control_flow: Vec<ControlFlow>,
}

pub struct Emitter<'a> {
    pub container: &'a str,
    pub methods: &'a HashSet<&'a str>,
    pub imports: &'a Imports,
    pub out: TemplateOut,
}

impl Emitter<'_> {
    pub fn walk(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Element(el) => self.element(el),
                Node::Interpolation { line, content } => {
                    self.expr_calls(content, *line, false);
                    self.expr_strings(content, *line);
                }
                Node::Text {
                    line,
                    raw,
                    decode: dec,
                } => {
                    let text = if *dec { decode(raw) } else { raw.clone() };
                    let value = collapse(text.trim());
                    if !value.is_empty() {
                        self.string(*line, truncate(&value, 300));
                    }
                }
            }
        }
    }

    fn element(&mut self, el: &Element) {
        let pas = pascal(&el.tag);
        let imp = self.imports.get(&el.tag).or_else(|| self.imports.get(&pas));
        if imp.is_some() || el.tag.starts_with(|c: char| c.is_ascii_uppercase()) {
            let target = match imp {
                Some(_) if self.imports.get(&el.tag).is_some() => el.tag.clone(),
                Some(_) => pas,
                None => el.tag.clone(),
            };
            let mut call = self.call(el.line, "component", target, "component_use");
            call.resolved = imp.map(str::to_string);
            self.out.calls.push(call);
        }
        for prop in &el.props {
            match prop {
                Prop::Dir {
                    line,
                    name,
                    arg,
                    exp,
                } => self.directive(el, *line, name, arg, exp),
                Prop::Attr {
                    line,
                    value: Some(v),
                    ..
                } if !v.trim().is_empty() => {
                    self.string(*line, truncate(v, 300));
                }
                Prop::Attr { .. } => {}
            }
        }
        self.walk(&el.children);
    }

    fn directive(
        &mut self,
        el: &Element,
        line: u32,
        name: &str,
        arg: &Option<String>,
        exp: &Option<String>,
    ) {
        let exp = exp.as_deref().filter(|e| !e.is_empty());
        if let Some(exp) = exp {
            self.expr_calls(exp, line, name == "on");
            self.expr_strings(exp, line);
        }
        if name == "on"
            && let Some(a) = arg.as_deref().filter(|a| !a.is_empty())
        {
            self.string(line, format!("@{a}"));
        }
        if let (true, Some(exp)) = (name == "if" || name == "else-if", exp) {
            let cond = collapse(exp);
            self.out.control_flow.push(ControlFlow {
                container: Some(self.container.to_string()),
                caller: Some("template".into()),
                kind: format!("v-{name}"),
                line: el.line,
                cond: Some(truncate(&cond, 100)),
                branches: vec![Branch {
                    label: format!("v-{name}=\"{}\"", truncate(&cond, 60)),
                    start_line: el.line,
                    end_line: el.end_line,
                    fall_through: None,
                }],
            });
        }
    }

    fn expr_calls(&mut self, exp: &str, line: u32, handler: bool) {
        if exp.is_empty() {
            return;
        }
        let found = expr_calls(exp);
        for m in &found {
            let head = m.name.split('.').next().unwrap_or("");
            let mut call = self.call(line, "func_call", m.name.clone(), "");
            let (category, prefix) = match head {
                "$t" | "t" | "$tc" => ("i18n_helper", "i18n::"),
                "route" => ("route_helper", "route::"),
                "$emit" | "emit" => ("vue_emit", "vue::emit:"),
                _ => ("", ""),
            };
            if !category.is_empty() {
                call.category = category.into();
                call.resolved = m.literal.as_ref().map(|l| format!("{prefix}{l}"));
            } else if OBJECT_PROTO.contains(&head) {
                // a JS-ben a prototípus-tulajdonság felülírja a kategóriát (üres marad)
            } else if self.methods.contains(head) {
                call.category = "internal_helper".into();
            } else if let Some(imp) = self.imports.get(head) {
                call.category = "module_use".into();
                let rest = if m.name.contains('.') {
                    &m.name[head.len()..]
                } else {
                    ""
                };
                call.resolved = Some(format!("{imp}{rest}"));
            } else {
                call.category = "external_call".into();
            }
            if m.literal.is_some() {
                call.args = m.quoted.clone().map(|q| vec![q]);
            }
            self.out.calls.push(call);
        }
        let trimmed = exp.trim();
        if handler && found.is_empty() && is_ident(trimmed) && self.methods.contains(trimmed) {
            let mut call = self.call(line, "func_call", trimmed.to_string(), "internal_helper");
            call.handler = Some(true);
            self.out.calls.push(call);
        }
    }

    /// A kifejezés (`{{ … }}`, `:attr="…"`, `@x="…"`, `v-if="…"`) string-literáljai; a sor a kifejezés
    /// kezdősora + a literál előtti sortörések.
    fn expr_strings(&mut self, exp: &str, line: u32) {
        for (offset, value) in expr_strings(exp) {
            let breaks = exp[..offset].matches('\n').count() as u32;
            self.string(line + breaks, truncate(&value, 300));
        }
    }

    fn call(&self, line: u32, kind: &str, target: String, category: &str) -> Call {
        Call {
            container: Some(self.container.to_string()),
            caller: Some("template".into()),
            line,
            end_line: None,
            kind: kind.into(),
            target,
            resolved: None,
            on: None,
            category: category.into(),
            args: None,
            handler: None,
        }
    }

    fn string(&mut self, line: u32, value: String) {
        self.out.strings.push(StringLit {
            container: Some(self.container.to_string()),
            caller: Some("template".into()),
            line,
            end_line: None,
            value,
        });
    }
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// `replace(/\s+/g, ' ')` (nem vág).
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}
