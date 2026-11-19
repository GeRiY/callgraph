//! The React emitter: the `Member`s of a `react:Name` / `hook:useX` expression as a function component or a custom
//! hook, TypeScript or plain JS, with or without semicolons. The members keep the Vue generator's spelling:
//! `props(…)` -> typed props (an `interface NameProps` + the destructured parameter with defaults), `emits(save:int)`
//! -> an optional `onSave` callback prop, `ref:n:T=v` -> `useState`, `computed:` -> a derived `const`, `on:mounted{…}`
//! -> `useEffect(…, [])`, `form(…)` -> Inertia's `useForm`; `fn:` / `const:` / `type:` / `raw{…}` as in a module.

use super::jsx;
use super::model::{Element, Import, Member, Prop};
use super::script::{IND, ScriptEmitter};
use super::types::{default_literal, ts_type};
use crate::generate::ast::TypeRef;
use crate::generate::error::GenResult;

/// The generator settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReactEmitter {
    pub ts: bool,
    pub semi: bool,
}

/// A prop of the component: the TS type text, optional, and the destructuring default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropDecl {
    pub name: String,
    pub ty: String,
    pub optional: bool,
    pub default: Option<String>,
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

/// `save` -> `onSave`, `update:value` / `row-click` -> `onUpdateValue` / `onRowClick`.
fn callback_name(event: &str) -> String {
    let pascal: String = event
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(capitalize)
        .collect();
    format!("on{pascal}")
}

/// The state setter of a `ref:` member: `count` -> `setCount`.
pub fn setter_name(name: &str) -> String {
    format!("set{}", capitalize(name))
}

/// The props of the members (`props(…)`, then the `emits(…)` callbacks), in order.
pub fn props_of(members: &[Member]) -> Vec<PropDecl> {
    let mut out: Vec<PropDecl> = Vec::new();
    let mut add = |decl: PropDecl| {
        if !out.iter().any(|p| p.name == decl.name) {
            out.push(decl);
        }
    };
    for m in members {
        match m {
            Member::Props(props) => {
                for p in props {
                    add(prop_decl(p));
                }
            }
            Member::Emits(emits) => {
                for e in emits {
                    let ty = match &e.payload {
                        Some(t) => format!("(payload: {}) => void", ts_type(t)),
                        None => "() => void".to_owned(),
                    };
                    add(PropDecl {
                        name: callback_name(&e.name),
                        ty,
                        optional: true,
                        default: None,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn prop_decl(p: &Prop) -> PropDecl {
    let ty = if p.name == "children" {
        "ReactNode".to_owned()
    } else {
        p.ty.as_ref().map_or_else(|| "unknown".to_owned(), ts_type)
    };
    PropDecl {
        name: p.name.clone(),
        ty,
        optional: !p.required,
        default: p.default.clone(),
    }
}

/// `{ a, b = 3 }` of the destructured parameter.
fn destructure(props: &[PropDecl]) -> String {
    let parts: Vec<String> = props
        .iter()
        .map(|p| match &p.default {
            Some(d) => format!("{} = {d}", p.name),
            None => p.name.clone(),
        })
        .collect();
    format!("{{ {} }}", parts.join(", "))
}

/// The imports the members need (`useState`, `useEffect`, `ReactNode`, `useForm`).
pub fn needed_imports(members: &[Member], ts: bool) -> Vec<Import> {
    let mut react: Vec<String> = Vec::new();
    let mut inertia: Vec<String> = Vec::new();
    let push = |list: &mut Vec<String>, name: &str| {
        if !list.iter().any(|n| n == name) {
            list.push(name.to_owned());
        }
    };
    for m in members {
        match m {
            Member::Ref { .. } => push(&mut react, "useState"),
            Member::Hook { .. } => push(&mut react, "useEffect"),
            Member::Form { .. } => push(&mut inertia, "useForm"),
            _ => {}
        }
    }
    if ts && props_of(members).iter().any(|p| p.ty == "ReactNode") {
        push(&mut react, "type ReactNode");
    }
    let mut out = Vec::new();
    for (source, named) in [("react", react), ("@inertiajs/react", inertia)] {
        if !named.is_empty() {
            out.push(Import {
                source: source.to_owned(),
                default: None,
                named,
            });
        }
    }
    out
}

impl ReactEmitter {
    fn end(&self) -> &'static str {
        if self.semi { ";" } else { "" }
    }

    fn script(&self) -> ScriptEmitter {
        ScriptEmitter {
            ts: self.ts,
            semi: self.semi,
        }
    }

    /// One member of a component / hook body.
    fn member(&self, m: &Member) -> Vec<String> {
        let end = self.end();
        match m {
            Member::Ref { name, ty, init } => {
                let init = init
                    .clone()
                    .or_else(|| ty.as_ref().and_then(default_literal).map(str::to_owned))
                    .unwrap_or_default();
                let generic = match (self.ts, ty) {
                    (true, Some(t)) => format!("<{}>", ts_type(t)),
                    _ => String::new(),
                };
                vec![format!(
                    "const [{name}, {}] = useState{generic}({init}){end}",
                    setter_name(name)
                )]
            }
            Member::Computed { name, ty, body } => {
                self.computed(name, ty.as_ref(), body.as_deref())
            }
            Member::Hook { body, .. } => {
                let mut lines = vec!["useEffect(() => {".to_owned()];
                lines.extend(body_lines(body.as_deref()));
                lines.push(format!("}}, []){end}"));
                lines
            }
            Member::Exported { member, .. } => self.member(member),
            other => self.script().member(other),
        }
    }

    fn computed(&self, name: &str, ty: Option<&TypeRef>, body: Option<&str>) -> Vec<String> {
        let end = self.end();
        let annotation = match (self.ts, ty) {
            (true, Some(t)) => format!(": {}", ts_type(t)),
            _ => String::new(),
        };
        let text = body.unwrap_or("").trim();
        let block = text.is_empty()
            || text.contains('\n')
            || text.contains(';')
            || text.starts_with("return ");
        if !block {
            return vec![format!("const {name}{annotation} = {text}{end}")];
        }
        let mut lines = vec![format!("const {name}{annotation} = (() => {{")];
        lines.extend(body_lines(body));
        lines.push(format!("}})(){end}"));
        lines
    }

    /// The members of a body, a blank line between them (adjacent one-line `useState`s stay together).
    pub fn members(&self, members: &[&Member]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut previous_ref = false;
        for (i, m) in members.iter().enumerate() {
            let lines = self.member(m);
            let single_ref = matches!(m, Member::Ref { .. }) && lines.len() == 1;
            if i > 0 && !(single_ref && previous_ref) {
                out.push(String::new());
            }
            previous_ref = single_ref;
            out.extend(lines);
        }
        out
    }

    /// The prop declarations as lines: TS `interface NameProps { … }`; JS nothing (the JSDoc is on the function).
    fn interface(&self, name: &str, props: &[PropDecl]) -> Vec<String> {
        let end = self.end();
        let mut lines = vec![format!("interface {name}Props {{")];
        for p in props {
            let opt = if p.optional { "?" } else { "" };
            lines.push(format!("{IND}{}{opt}: {}{end}", p.name, p.ty));
        }
        lines.push("}".to_owned());
        lines
    }

    fn jsdoc(props: &[PropDecl]) -> Vec<String> {
        let fields: Vec<String> = props
            .iter()
            .map(|p| {
                let ty = if p.ty == "ReactNode" {
                    "import('react').ReactNode"
                } else {
                    p.ty.as_str()
                };
                let opt = if p.optional { "?" } else { "" };
                format!("{}{opt}: {ty}", p.name)
            })
            .collect();
        vec![format!(
            "/** @param {{{{ {} }}}} props */",
            fields.join(", ")
        )]
    }

    /// The component: hoisted types, the props interface, the function (importless).
    pub fn component(
        &self,
        name: &str,
        members: &[Member],
        template: Option<&[Element]>,
        default_export: bool,
    ) -> GenResult<Vec<String>> {
        let props = props_of(members);
        let (types, body): (Vec<&Member>, Vec<&Member>) = members
            .iter()
            .filter(|m| !matches!(m, Member::Props(_) | Member::Emits(_)))
            .partition(|m| matches!(m, Member::Type { .. } | Member::Alias { .. }));
        let mut lines: Vec<String> = Vec::new();
        for t in &types {
            lines.extend(self.script().member(t));
            lines.push(String::new());
        }
        let params = if props.is_empty() {
            String::new()
        } else if self.ts {
            lines.extend(self.interface(name, &props));
            lines.push(String::new());
            format!("{}: {name}Props", destructure(&props))
        } else {
            lines.extend(Self::jsdoc(&props));
            destructure(&props)
        };
        let export = if default_export {
            "export default "
        } else {
            "export "
        };
        lines.push(format!("{export}function {name}({params}) {{"));
        let mut inner = self.members(&body);
        if !inner.is_empty() {
            inner.push(String::new());
        }
        inner.extend(self.render(template)?);
        lines.extend(
            inner
                .into_iter()
                .map(|l| if l.is_empty() { l } else { format!("{IND}{l}") }),
        );
        lines.push("}".to_owned());
        Ok(lines)
    }

    /// `return (…);` (or `return null;` without a template).
    fn render(&self, template: Option<&[Element]>) -> GenResult<Vec<String>> {
        let end = self.end();
        let Some(elements) = template.filter(|e| !e.is_empty()) else {
            return Ok(vec![format!("return null{end}")]);
        };
        jsx::check(elements)?;
        let mut out = vec!["return (".to_owned()];
        out.extend(
            jsx::lines(elements)?
                .into_iter()
                .map(|l| format!("{IND}{l}")),
        );
        out.push(format!("){end}"));
        Ok(out)
    }

    /// A custom hook: hoisted (exported) types, `export function useX() { …; return { … } }`.
    pub fn hook(&self, name: &str, members: &[Member]) -> Vec<String> {
        let (types, body): (Vec<&Member>, Vec<&Member>) = members
            .iter()
            .partition(|m| matches!(m, Member::Type { .. } | Member::Alias { .. }));
        let mut names: Vec<String> = Vec::new();
        for m in &body {
            if let Member::Ref { name, .. } = m {
                names.push(name.clone());
                names.push(setter_name(name));
            } else if let Some(n) = m.declared_name() {
                names.push(n.to_owned());
            }
        }
        names.dedup();
        let mut inner = self.members(&body);
        if !inner.is_empty() {
            inner.push(String::new());
        }
        inner.extend(self.script().return_lines(&names));
        let mut lines = Vec::new();
        for t in &types {
            let mut decl = self.script().member(t);
            if self.ts {
                decl[0].insert_str(0, "export ");
            }
            lines.extend(decl);
            lines.push(String::new());
        }
        lines.push(format!("export function {name}() {{"));
        lines.extend(
            inner
                .into_iter()
                .map(|l| if l.is_empty() { l } else { format!("{IND}{l}") }),
        );
        lines.push("}".to_owned());
        lines
    }

    /// The JSX of a template as `return (…)` lines for an `--into`: just the elements, no wrapper.
    pub fn elements(&self, elements: &[Element]) -> GenResult<Vec<String>> {
        jsx::check(elements)?;
        let mut out = Vec::new();
        for e in elements {
            out.extend(jsx::lines(std::slice::from_ref(e))?);
        }
        Ok(out)
    }
}

/// The raw body lines (indented one level); no body: `// TODO`.
fn body_lines(body: Option<&str>) -> Vec<String> {
    let text = body.unwrap_or("").trim();
    if text.is_empty() {
        return vec![format!("{IND}// TODO")];
    }
    text.lines()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                format!("{IND}{l}")
            }
        })
        .collect()
}
