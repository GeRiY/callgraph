//! A hívott kifejezés feloldása kategóriára (`resolveCallee`) és a tagelérési lánc gyökere (`memberRoot`).

use std::collections::{HashMap, HashSet};

use oxc_ast::ast::Expression;

use super::receivers::TypeNames;

const NATIVE_GLOBALS: &[&str] = &[
    "console",
    "window",
    "document",
    "Math",
    "JSON",
    "Object",
    "Array",
    "Promise",
    "Number",
    "String",
    "Boolean",
    "Date",
    "RegExp",
    "Map",
    "Set",
    "WeakMap",
    "URL",
    "URLSearchParams",
    "FormData",
    "Intl",
    "Reflect",
    "localStorage",
    "sessionStorage",
    "navigator",
    "location",
    "history",
    "setTimeout",
    "clearTimeout",
    "setInterval",
    "clearInterval",
    "requestAnimationFrame",
    "cancelAnimationFrame",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "encodeURIComponent",
    "decodeURIComponent",
    "encodeURI",
    "decodeURI",
    "fetch",
    "alert",
    "confirm",
    "prompt",
    "structuredClone",
    "queueMicrotask",
    "Error",
    "TypeError",
    "Symbol",
    "BigInt",
    "Blob",
    "File",
    "FileReader",
    "Image",
    "IntersectionObserver",
    "ResizeObserver",
    "MutationObserver",
    "CustomEvent",
    "Event",
];
const NATIVE_GLOBALS_2: &[&str] = &[
    "AbortController",
    "atob",
    "btoa",
    "require",
    "globalThis",
    "performance",
    "crypto",
    "getComputedStyle",
    "matchMedia",
];
const VUE_MACROS: &[&str] = &[
    "defineProps",
    "defineEmits",
    "defineExpose",
    "withDefaults",
    "defineModel",
    "defineOptions",
    "defineSlots",
];

pub fn is_native(name: &str) -> bool {
    NATIVE_GLOBALS.contains(&name) || NATIVE_GLOBALS_2.contains(&name) || name == "scrollTo"
}

/// Egy helyi név kötése (import, `const x = hívás()`, `{emit}` paraméter).
#[derive(Debug, Clone)]
pub struct Binding {
    pub resolved: String,
    pub category: &'static str,
    /// A nem helyi import forrása, amelyből a név (közvetve) jön (`Call::from`).
    pub from: Option<String>,
}

/// Egy feloldott hívás.
#[derive(Debug, Clone)]
pub struct Callee {
    pub target: String,
    pub on: Option<String>,
    pub resolved: Option<String>,
    pub category: &'static str,
    pub kind: &'static str,
    /// A nem helyi import forrása (`Call::from`).
    pub from: Option<String>,
}

impl Callee {
    fn new(
        target: &str,
        on: Option<&str>,
        resolved: Option<String>,
        category: &'static str,
        kind: &'static str,
    ) -> Self {
        Self {
            target: target.to_string(),
            on: on.map(str::to_string),
            resolved,
            category,
            kind,
            from: None,
        }
    }

    /// Az ismeretlen hívott kifejezés.
    pub fn expr() -> Self {
        Self::new("<expr>", None, None, "external_call", "func_call")
    }
}

/// A feloldás környezete: metódusnevek, fájlon belüli függvények, kötések.
#[derive(Default)]
pub struct Scope {
    pub names: HashSet<String>,
    pub local_fns: HashSet<String>,
    pub bindings: HashMap<String, Binding>,
    /// A `import * as ns from './x'` névterek helyi nevei (az `ns.fn` értékként átadva hivatkozás).
    pub namespaces: HashSet<String>,
    /// A fogadó-típusok feloldásához ismert típusnevek (osztályok, interfészek, importok).
    pub types: TypeNames,
}

/// A tagelérési lánc gyökere.
pub enum Root<'b> {
    Ident(&'b str),
    This,
    Other,
}

/// A literál tagelérési kulcs szövege: string, helyettesítés nélküli sablon, szám.
fn literal_key(e: &Expression<'_>) -> Option<String> {
    match e.get_inner_expression() {
        Expression::StringLiteral(s) => Some(s.value.display().to_string()),
        Expression::NumericLiteral(n) => Some(n.value.to_string()),
        Expression::TemplateLiteral(t) if t.is_no_substitution_template() => {
            t.quasis.first().map(|q| {
                q.value
                    .cooked
                    .as_ref()
                    .map_or_else(|| q.value.raw.to_string(), |c| c.display().to_string())
            })
        }
        _ => None,
    }
}

/// `a.b[c].d` -> gyökér + `["b","[]","d"]`.
pub fn member_root<'a, 'b>(mut e: &'b Expression<'a>) -> (Root<'b>, Vec<String>) {
    let mut parts = Vec::new();
    loop {
        e = e.without_parentheses();
        match e {
            Expression::StaticMemberExpression(m) => {
                parts.push(m.property.name.to_string());
                e = &m.object;
            }
            Expression::ComputedMemberExpression(m) => {
                // literál kulcs (`o['x']`, ``o[`x`]``, `a[0]`) = a név, mint a `.x`; a nem literál kulcs
                // (`o[k]`) csak láncközi elem, a hívás célpontjaként a hívásbejáró dinamikusnak jelöli
                parts.push(literal_key(&m.expression).unwrap_or_else(|| "[]".to_string()));
                e = &m.object;
            }
            Expression::PrivateFieldExpression(m) => {
                parts.push(String::new());
                e = &m.object;
            }
            Expression::Identifier(i) => {
                parts.reverse();
                return (Root::Ident(i.name.as_str()), parts);
            }
            Expression::ThisExpression(_) => {
                parts.reverse();
                return (Root::This, parts);
            }
            _ => {
                parts.reverse();
                return (Root::Other, parts);
            }
        }
    }
}

/// Kisbetűvel (vagy `_`-lel) kezdődő név: függvény-konvenció (az osztály / komponens / konstans nem hivatkozás).
fn is_fn_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_lowercase() || c == '_')
}

impl Scope {
    /// Értékként átadott függvény (`guarded(rest.listBeds)`, `[a, b]`, `{ k: handler }`): a fájl saját függvénye, a
    /// fájlon belüli objektum-literál metódusa, helyi import vagy névtér-import tagja. Más (változó, csomag,
    /// osztály, konstans) nem hivatkozás — a kötés a feloldás nélküli `Callee`-t adja vissza.
    pub fn reference(&self, e: &Expression<'_>) -> Option<Callee> {
        match e.without_parentheses() {
            Expression::Identifier(i) => {
                let nm = i.name.as_str();
                if let Some(b) = self.bindings.get(nm) {
                    let import =
                        b.from.is_none() && !b.resolved.contains('(') && b.category == "module_use";
                    return (import && is_fn_name(nm)).then(|| self.ident(nm));
                }
                self.local_fns.contains(nm).then(|| self.ident(nm))
            }
            m @ Expression::StaticMemberExpression(s) => {
                let Expression::Identifier(o) = s.object.without_parentheses() else {
                    return None;
                };
                let (obj, prop) = (o.name.as_str(), s.property.name.as_str());
                if !is_fn_name(prop) {
                    return None;
                }
                let own_object = !self.bindings.contains_key(obj)
                    && self.names.contains(&format!("{obj}.{prop}"));
                let namespace = self.namespaces.contains(obj)
                    && self.bindings.get(obj).is_some_and(|b| b.from.is_none());
                (own_object || namespace).then(|| self.member(m))
            }
            _ => None,
        }
    }

    pub fn resolve(&self, callee: &Expression<'_>) -> Callee {
        match callee.without_parentheses() {
            Expression::Identifier(i) => self.ident(i.name.as_str()),
            e @ (Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_)
            | Expression::PrivateFieldExpression(_)) => self.member(e),
            _ => Callee::expr(),
        }
    }

    fn ident(&self, nm: &str) -> Callee {
        let f = |resolved: Option<String>, category| {
            Callee::new(nm, None, resolved, category, "func_call")
        };
        if self.names.contains(nm) {
            return f(None, "internal_helper");
        }
        if let Some(b) = self.bindings.get(nm) {
            let mut callee = f(Some(b.resolved.clone()), b.category);
            callee.from.clone_from(&b.from);
            return callee;
        }
        if self.local_fns.contains(nm) {
            return f(None, "internal_helper");
        }
        if VUE_MACROS.contains(&nm) {
            return f(Some(format!("vue::{nm}")), "vue_macro");
        }
        match nm {
            "route" => f(Some("ziggy::route".into()), "route_helper"),
            "t" | "$t" => f(None, "i18n_helper"),
            _ if is_native(nm) => f(None, "native_js"),
            _ => f(None, "global_helper"),
        }
    }

    fn member(&self, e: &Expression<'_>) -> Callee {
        let (root, parts) = member_root(e);
        let last = parts.last().cloned().unwrap_or_default();
        let joined = parts.join(".");
        let m = "method_call";
        match root {
            Root::Ident(nm) => {
                let tgt = format!("{nm}.{joined}");
                if let Some(b) = self.bindings.get(nm) {
                    // `const x = computed(…)` / `ref(…)`: az `x.value.…` már a saját adat (tömb-, string-metódus),
                    // nem a Vue API
                    if b.category == "vue_api"
                        && b.resolved.contains("()")
                        && parts.len() > 1
                        && parts.first().is_some_and(|p| p == "value")
                    {
                        return Callee::new(&last, Some(nm), None, "external_call", m);
                    }
                    let resolved = format!("{}.{joined}", b.resolved);
                    let mut callee = Callee::new(&tgt, Some(nm), Some(resolved), b.category, m);
                    callee.from.clone_from(&b.from);
                    return callee;
                }
                if is_native(nm) {
                    return Callee::new(&tgt, Some(nm), None, "native_js", m);
                }
                // a fájl objektum-literáljának metódusa (`api.fetch()` → `api.fetch`)
                if self.names.contains(&tgt) {
                    return Callee::new(&tgt, Some(nm), None, "internal_helper", m);
                }
                Callee::new(&last, Some(nm), None, "external_call", m)
            }
            Root::This => {
                let p0 = parts.first().map(String::as_str).unwrap_or("");
                let this = Some("this");
                if parts.len() == 1 && self.names.contains(p0) {
                    return Callee::new(p0, this, None, "internal_helper", m);
                }
                match p0 {
                    "$inertia" | "$page" => Callee::new(
                        &format!("this.{joined}"),
                        this,
                        Some(format!("@inertiajs/vue3::{joined}")),
                        "inertia",
                        m,
                    ),
                    "$t" | "$tc" => Callee::new(p0, this, None, "i18n_helper", m),
                    "$emit" => Callee::new("$emit", this, None, "vue_emit", m),
                    _ => Callee::new(&format!("this.{joined}"), this, None, "external_call", m),
                }
            }
            Root::Other => Callee::new(&last, Some("<expr>"), None, "external_call", m),
        }
    }
}
