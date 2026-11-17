//! Docblock-típusforrások a hívás-célpontok feloldásához (a natív típus mindig erősebb): `@mixin`, `@property`,
//! `@method`, `@param`, `@return` és a változó / foreach `@var`-ja. Csak az egyértelmű osztálytípus számít
//! (`?Book`, `Book|null` → `Book`; több osztályos unió, generikus, skalár nem).

use super::{helpers::Collector, state::State};
use crate::lang::php::analysis::docblock::{
    class_type, doc_comment_before, element_class, method_tags, mixin_tags, param_tags,
    property_tags, return_tag, var_tags,
};
use crate::lang::php::analysis::type_tracker::TypeTracker;
use crate::lang::php::printer::{lossy, strip_expr as strip};
use mago_span::HasSpan;
use mago_syntax::cst::*;
use std::collections::{HashMap, HashSet};

/// A Laravel `JsonResource` / `ResourceCollection` saját metódusai (kisbetűvel): a `@mixin` modell nem kapja meg.
const RESOURCE_OWN: &[&str] = &[
    "when",
    "whenloaded",
    "whencounted",
    "whenaggregated",
    "whenpivotloaded",
    "whenpivotloadedas",
    "whennotnull",
    "whenhas",
    "whenexistsloaded",
    "merge",
    "mergewhen",
    "mergeunless",
    "unless",
    "resolve",
    "toarray",
    "toresponse",
    "additional",
    "with",
    "response",
    "withresponse",
    "jsonserialize",
    "collection",
    "make",
];

/// Az aktuális osztály docblock-ból és tagjaiból gyűjtött adatai.
#[derive(Default)]
pub(super) struct ClassDocs {
    /// Az egyetlen `@mixin` feloldott osztálya (több vagy nincs: `None`).
    pub(super) mixin: Option<String>,
    /// Az osztályban definiált és a `@method`-dal deklarált metódusok (kisbetűvel).
    pub(super) own: HashSet<String>,
    /// A csak `@method`-dal deklarált metódusok (kisbetűvel).
    pub(super) declared: HashSet<String>,
    /// Metódus (kisbetűvel) → a visszatérési osztály (natív típus, különben `@return` / `@method`).
    pub(super) returns: HashMap<String, String>,
}

impl Collector<'_, '_> {
    /// Egy osztály-szerű belépésekor: a `@mixin` / `@method` / `@property` tagek és a metódusok visszatérési típusai.
    /// A `start` a deklaráció kezdő offsetje (a docblock közvetlenül előtte áll).
    pub(super) fn enter_class_docs(
        &self,
        s: &mut State,
        members: &Sequence<'_, ClassLikeMember<'_>>,
        start: u32,
    ) {
        let mut docs = ClassDocs::default();
        for m in members.iter() {
            let ClassLikeMember::Method(m) = m else {
                continue;
            };
            let name = lossy(m.name.value).to_ascii_lowercase();
            let ret = match &m.return_type_hint {
                Some(r) => class_type(self.parsed.text(r.hint.span()).trim()),
                None => doc_comment_before(self.parsed, m.span().start.offset)
                    .and_then(|raw| return_tag(&raw))
                    .and_then(|ty| class_type(&ty)),
            };
            if let Some(ty) = ret {
                docs.returns
                    .insert(name.clone(), s.resolver.resolve_class(&ty));
            }
            docs.own.insert(name);
        }
        if let Some(raw) = doc_comment_before(self.parsed, start) {
            let mixins = mixin_tags(&raw);
            if let [only] = mixins.as_slice()
                && let Some(ty) = class_type(only)
            {
                docs.mixin = Some(s.resolver.resolve_class(&ty));
            }
            for (name, ret) in method_tags(&raw) {
                let lower = name.to_ascii_lowercase();
                if docs.own.insert(lower.clone()) {
                    docs.declared.insert(lower.clone());
                }
                if let Some(ty) = ret.as_deref().and_then(class_type) {
                    docs.returns
                        .entry(lower)
                        .or_insert_with(|| s.resolver.resolve_class(&ty));
                }
            }
            let class = s.resolver.resolve_class("self").to_ascii_lowercase();
            for (name, ty) in property_tags(&raw) {
                let Some(ty) = class_type(&ty).map(|t| s.resolver.resolve_class(&t)) else {
                    continue;
                };
                let this = format!("this->{name}");
                if s.tracker.type_of(&this).is_none() {
                    s.tracker.set(this, ty.clone());
                }
                s.class_props
                    .entry(format!("{class}::{name}"))
                    .or_insert(ty);
            }
        }
        s.docs = docs;
    }

    /// `@param Book $book` a metódus docblockjából a natív típus nélküli paraméterekhez.
    pub(super) fn doc_param_types(&self, s: &mut State, n: &Method<'_>) {
        if n.parameter_list.parameters.iter().all(|p| p.hint.is_some()) {
            return;
        }
        let Some(raw) = doc_comment_before(self.parsed, n.span().start.offset) else {
            return;
        };
        let tags = param_tags(&raw);
        for p in n
            .parameter_list
            .parameters
            .iter()
            .filter(|p| p.hint.is_none())
        {
            let name = lossy(p.variable.name);
            let found = tags.iter().find(|(var, _)| *var == name);
            if let Some(ty) = found.and_then(|(_, ty)| class_type(ty)) {
                s.tracker.set(name, s.resolver.resolve_class(&ty));
            }
        }
    }

    /// A `$var` típusa a `start` előtti docblock `@var` tagjából (a névtelen tag az értékadás célpontjára vonatkozik).
    pub(super) fn var_doc_type(&self, s: &State, start: u32, var: &str) -> Option<String> {
        let raw = doc_comment_before(self.parsed, start)?;
        var_tags(&raw)
            .into_iter()
            .filter(|(_, name)| name.as_deref().is_none_or(|n| n == var))
            .find_map(|(ty, _)| class_type(&ty))
            .map(|ty| s.resolver.resolve_class(&ty))
    }

    /// `foreach ($books as $b)`: a `$b` típusa a `@var Book $b` tagből, vagy a forrás `@var Collection<Book> $books`
    /// (`Book[]`, `list<Book>`) elemtípusából.
    pub(super) fn foreach_var_types(&self, s: &mut State, n: &Foreach<'_>) {
        let value = match &n.target {
            ForeachTarget::Value(t) => t.value,
            ForeachTarget::KeyValue(t) => t.value,
        };
        let Expression::Variable(Variable::Direct(v)) = strip(value) else {
            return;
        };
        let Some(raw) = doc_comment_before(self.parsed, n.span().start.offset) else {
            return;
        };
        let source = match strip(n.expression) {
            Expression::Variable(Variable::Direct(x)) => Some(lossy(x.name)),
            _ => None,
        };
        let value_name = lossy(v.name);
        let mut found = None;
        for (ty, name) in var_tags(&raw) {
            let element = match name.as_deref() {
                Some(name) if name == value_name => class_type(&ty),
                Some(name) if Some(name) == source.as_deref() => element_class(&ty),
                None => class_type(&ty),
                Some(_) => None,
            };
            if element.is_some() {
                found = element;
                break;
            }
        }
        if let Some(ty) = found {
            s.tracker.set(value_name, s.resolver.resolve_class(&ty));
        }
    }

    /// `$this->m(...)` visszatérési típusa az osztály saját docblockja / natív típusa szerint.
    pub(super) fn this_call_type(&self, s: &State, e: &Expression<'_>) -> Option<String> {
        let Expression::Call(Call::Method(c)) = strip(e) else {
            return None;
        };
        let Expression::Variable(Variable::Direct(v)) = strip(c.object) else {
            return None;
        };
        let ClassLikeMemberSelector::Identifier(m) = &c.method else {
            return None;
        };
        if v.name != b"$this" {
            return None;
        }
        s.docs
            .returns
            .get(&lossy(m.value).to_ascii_lowercase())
            .cloned()
    }

    /// A `$x->book` property típusa, ha az `$x` típusa ebben a fájlban `@property`-vel deklarálja.
    pub(super) fn doc_property_type(
        &self,
        s: &State,
        object: &Expression<'_>,
        property: &str,
    ) -> Option<String> {
        let Expression::Variable(Variable::Direct(v)) = strip(object) else {
            return None;
        };
        let class = s.tracker.type_of(&lossy(v.name))?.to_ascii_lowercase();
        s.class_props.get(&format!("{class}::{property}")).cloned()
    }

    /// A `$this->m()` hívás célja, ha nem az osztály valódi metódusa: a `@method`-dal deklarált metódus az osztályé,
    /// a sehol (az osztályban) nem definiált pedig a `@mixin` osztályé.
    pub(super) fn this_method_target(
        &self,
        s: &State,
        object: &Expression<'_>,
        method: &str,
    ) -> Option<String> {
        let Expression::Variable(Variable::Direct(v)) = strip(object) else {
            return None;
        };
        if v.name != b"$this" || method.starts_with("__") {
            return None;
        }
        let lower = method.to_ascii_lowercase();
        if s.docs.declared.contains(&lower) {
            return Some(format!("{}::{method}", s.resolver.resolve_class("self")));
        }
        if s.docs.own.contains(&lower) {
            return None;
        }
        // a Laravel `JsonResource` saját metódusait nem továbbítja a modellnek
        if RESOURCE_OWN.contains(&lower.as_str()) {
            return None;
        }
        s.docs
            .mixin
            .as_ref()
            .map(|mixin| format!("{mixin}::{method}"))
    }
}
