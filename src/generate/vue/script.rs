//! A script-emitter (Strategy): a `Member`-ekből `<script setup>` / composable sorok, TypeScript vagy sima JS
//! változatban, pontosvesszővel vagy anélkül. 2 szóköz behúzás, aposztróf. A tagok között egy üres sor
//! (a szomszédos egysoros `ref`-ek kivételével).

use std::collections::BTreeSet;

use super::model::{Emit, FormField, Import, Member, Prop, VParam};
use super::types::{default_literal, runtime_type, ts_type};
use crate::generate::ast::TypeRef;

/// Behúzás egy szinten.
pub const IND: &str = "  ";

/// A script-kódgenerátor beállításai.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptEmitter {
    pub ts: bool,
    pub semi: bool,
}

/// A nyers törzs sorai a megadott behúzással; törzs nélkül `// TODO`.
fn body_lines(body: Option<&str>, indent: &str) -> Vec<String> {
    let text = body.unwrap_or("").trim();
    if text.is_empty() {
        return vec![format!("{indent}// TODO")];
    }
    text.lines()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                format!("{indent}{l}")
            }
        })
        .collect()
}

fn text_lines(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_owned).collect()
}

/// A tömb / objektum alapértékhez gyár-függvény kell (`default: () => []`).
fn factory(default: &str) -> String {
    if default.starts_with('[') || default.starts_with('{') {
        format!("() => {default}")
    } else {
        default.to_owned()
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

impl ScriptEmitter {
    fn end(&self) -> &'static str {
        if self.semi { ";" } else { "" }
    }

    /// A tagok sorai, egy üres sorral elválasztva.
    pub fn members(&self, members: &[Member]) -> Vec<String> {
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

    fn member(&self, m: &Member) -> Vec<String> {
        match m {
            Member::Props(props) => self.props(props),
            Member::Emits(emits) => self.emits(emits),
            Member::Ref { name, ty, init } => self.reference(name, ty.as_ref(), init.as_deref()),
            Member::Computed { name, ty, body } => {
                self.computed(name, ty.as_ref(), body.as_deref())
            }
            Member::Func {
                name,
                params,
                ret,
                body,
                is_async,
            } => self.function(name, params, ret.as_ref(), body.as_deref(), *is_async),
            Member::Form { name, fields } => self.form(name, fields),
            Member::Const {
                name,
                ty,
                value,
                mutable,
            } => self.constant(name, ty.as_ref(), value.as_deref(), *mutable),
            Member::Hook { name, body } => self.hook(name, body.as_deref()),
        }
    }

    fn props(&self, props: &[Prop]) -> Vec<String> {
        let end = self.end();
        let mut lines = Vec::new();
        if self.ts {
            let defaults = props.iter().any(|p| p.default.is_some());
            lines.push(if defaults {
                "const props = withDefaults(defineProps<{".to_owned()
            } else {
                "const props = defineProps<{".to_owned()
            });
            for p in props {
                let ty = p.ty.as_ref().map_or_else(|| "unknown".to_owned(), ts_type);
                let opt = if p.required { "" } else { "?" };
                lines.push(format!("{IND}{}{opt}: {ty}{end}", p.name));
            }
            if defaults {
                lines.push("}>(), {".to_owned());
                for p in props {
                    if let Some(d) = &p.default {
                        lines.push(format!("{IND}{}: {},", p.name, factory(d)));
                    }
                }
                lines.push(format!("}}){end}"));
            } else {
                lines.push(format!("}}>(){end}"));
            }
        } else {
            lines.push("const props = defineProps({".to_owned());
            for p in props {
                let ty =
                    p.ty.as_ref()
                        .map_or_else(|| "null".to_owned(), runtime_type);
                let mut parts = vec![format!("type: {ty}")];
                if p.required {
                    parts.push("required: true".to_owned());
                }
                if let Some(d) = &p.default {
                    parts.push(format!("default: {}", factory(d)));
                }
                lines.push(format!("{IND}{}: {{ {} }},", p.name, parts.join(", ")));
            }
            lines.push(format!("}}){end}"));
        }
        lines
    }

    fn emits(&self, emits: &[Emit]) -> Vec<String> {
        let end = self.end();
        if self.ts {
            let mut lines = vec!["const emit = defineEmits<{".to_owned()];
            for e in emits {
                let args = e
                    .payload
                    .as_ref()
                    .map_or_else(String::new, |t| format!("payload: {}", ts_type(t)));
                lines.push(format!("{IND}{}: [{args}]{end}", e.name));
            }
            lines.push(format!("}}>(){end}"));
            lines
        } else {
            let names: Vec<String> = emits.iter().map(|e| format!("'{}'", e.name)).collect();
            vec![format!(
                "const emit = defineEmits([{}]){end}",
                names.join(", ")
            )]
        }
    }

    fn reference(&self, name: &str, ty: Option<&TypeRef>, init: Option<&str>) -> Vec<String> {
        let end = self.end();
        let init = init
            .map(str::to_owned)
            .or_else(|| ty.and_then(default_literal).map(str::to_owned));
        let generic = match (self.ts, ty) {
            (true, Some(t)) => format!("<{}>", ts_type(t)),
            _ => String::new(),
        };
        text_lines(&format!(
            "const {name} = ref{generic}({}){end}",
            init.unwrap_or_default()
        ))
    }

    fn computed(&self, name: &str, ty: Option<&TypeRef>, body: Option<&str>) -> Vec<String> {
        let end = self.end();
        let generic = match (self.ts, ty) {
            (true, Some(t)) => format!("<{}>", ts_type(t)),
            _ => String::new(),
        };
        let text = body.unwrap_or("").trim();
        let block = text.is_empty()
            || text.contains('\n')
            || text.contains(';')
            || text.starts_with("return ");
        if block {
            let mut lines = vec![format!("const {name} = computed{generic}(() => {{")];
            lines.extend(body_lines(body, IND));
            lines.push(format!("}}){end}"));
            lines
        } else {
            vec![format!(
                "const {name} = computed{generic}(() => {text}){end}"
            )]
        }
    }

    fn param(&self, p: &VParam) -> String {
        let mut text = String::new();
        if p.rest {
            text.push_str("...");
        }
        text.push_str(&p.name);
        if self.ts {
            if p.optional && p.default.is_none() {
                text.push('?');
            }
            if let Some(t) = &p.ty {
                text.push_str(&format!(": {}", ts_type(t)));
            }
        }
        if let Some(d) = &p.default {
            text.push_str(&format!(" = {d}"));
        }
        text
    }

    fn function(
        &self,
        name: &str,
        params: &[VParam],
        ret: Option<&TypeRef>,
        body: Option<&str>,
        is_async: bool,
    ) -> Vec<String> {
        let params: Vec<String> = params.iter().map(|p| self.param(p)).collect();
        let ret = match (self.ts, ret) {
            (true, Some(t)) if is_async => format!(": Promise<{}>", ts_type(t)),
            (true, Some(t)) => format!(": {}", ts_type(t)),
            (true, None) if is_async => ": Promise<void>".to_owned(),
            _ => String::new(),
        };
        let prefix = if is_async { "async " } else { "" };
        let mut lines = vec![format!(
            "{prefix}function {name}({}){ret} {{",
            params.join(", ")
        )];
        lines.extend(body_lines(body, IND));
        lines.push("}".to_owned());
        lines
    }

    fn form(&self, name: &str, fields: &[FormField]) -> Vec<String> {
        let end = self.end();
        let mut lines = vec![format!("const {name} = useForm({{")];
        for f in fields {
            let value = f
                .default
                .clone()
                .or_else(|| f.ty.as_ref().and_then(default_literal).map(str::to_owned))
                .unwrap_or_else(|| "''".to_owned());
            lines.push(format!("{IND}{}: {value},", f.name));
        }
        lines.push(format!("}}){end}"));
        lines
    }

    fn constant(
        &self,
        name: &str,
        ty: Option<&TypeRef>,
        value: Option<&str>,
        mutable: bool,
    ) -> Vec<String> {
        let end = self.end();
        let keyword = if mutable { "let" } else { "const" };
        let annotation = match (self.ts, ty) {
            (true, Some(t)) => format!(": {}", ts_type(t)),
            _ => String::new(),
        };
        let value = value
            .map(str::to_owned)
            .or_else(|| ty.and_then(default_literal).map(str::to_owned));
        match value {
            Some(v) => text_lines(&format!("{keyword} {name}{annotation} = {v}{end}")),
            None if mutable => vec![format!("let {name}{annotation}{end}")],
            None => vec![format!("const {name}{annotation} = undefined{end}")],
        }
    }

    fn hook(&self, name: &str, body: Option<&str>) -> Vec<String> {
        let mut lines = vec![format!("{}(() => {{", hook_name(name))];
        lines.extend(body_lines(body, IND));
        lines.push(format!("}}){}", self.end()));
        lines
    }

    /// A tagok által igényelt importok (`ref`, `computed`, `onMounted` a `vue`-ból, `useForm` az Inertiából).
    pub fn needed_imports(members: &[Member]) -> Vec<Import> {
        let mut vue = BTreeSet::new();
        let mut inertia = BTreeSet::new();
        for m in members {
            match m {
                Member::Ref { .. } => {
                    vue.insert("ref".to_owned());
                }
                Member::Computed { .. } => {
                    vue.insert("computed".to_owned());
                }
                Member::Hook { name, .. } => {
                    vue.insert(hook_name(name));
                }
                Member::Form { .. } => {
                    inertia.insert("useForm".to_owned());
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        if !vue.is_empty() {
            out.push(Import {
                source: "vue".to_owned(),
                default: None,
                named: vue.into_iter().collect(),
            });
        }
        if !inertia.is_empty() {
            out.push(Import {
                source: "@inertiajs/vue3".to_owned(),
                default: None,
                named: inertia.into_iter().collect(),
            });
        }
        out
    }

    /// A composable `return { … }` blokkja a nevekkel (többsoros, záró vesszővel).
    pub fn return_lines(&self, names: &[String]) -> Vec<String> {
        let mut lines = vec!["return {".to_owned()];
        lines.extend(names.iter().map(|n| format!("{IND}{n},")));
        lines.push(format!("}}{}", self.end()));
        lines
    }

    /// A teljes composable-függvény (importok nélkül): `export function useX() { … return { … } }`.
    pub fn composable(&self, name: &str, members: &[Member]) -> Vec<String> {
        let mut body = self.members(members);
        let mut names: Vec<String> = Vec::new();
        for n in members.iter().filter_map(Member::declared_name) {
            if !names.iter().any(|x| x == n) {
                names.push(n.to_owned());
            }
        }
        if !body.is_empty() {
            body.push(String::new());
        }
        body.extend(self.return_lines(&names));
        let mut lines = vec![format!("export function {name}() {{")];
        lines.extend(
            body.into_iter()
                .map(|l| if l.is_empty() { l } else { format!("{IND}{l}") }),
        );
        lines.push("}".to_owned());
        lines
    }
}

/// `mounted` → `onMounted`; a már `onMounted` alakot meghagyja.
pub fn hook_name(name: &str) -> String {
    let already = name.strip_prefix("on").and_then(|r| r.chars().next());
    if already.is_some_and(char::is_uppercase) {
        name.to_owned()
    } else {
        format!("on{}", capitalize(name))
    }
}
