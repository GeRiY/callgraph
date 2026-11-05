//! Rust hívás-kategorizálás: a hívás alakja (`Callee`) + a fájl névtáblája → kategória és feloldott név.
//! A szabályok a `core::Chain`-en futnak (Chain of Responsibility), az első illeszkedő dönt.

mod data;
mod rules;

use crate::core::{Chain, Rule};

use super::resolve::Resolver;

/// A hívott fél alakja (syn-független).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Callee {
    /// `foo()`, `Foo::new()`, `std::fs::read()` — az út szegmensei (generikus argumentumok nélkül).
    Path(Vec<String>),
    /// `x.bar()` — a metódus neve, és hogy a fogadó pontosan `self`-e.
    Method { name: String, on_self: bool },
    /// `println!()`, `sqlx::query!()` — a makró útja.
    Macro(Vec<String>),
}

impl Callee {
    /// A kimenet `type` mezője.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Callee::Path(segs) if segs.len() >= 2 => {
                let owner = &segs[segs.len() - 2];
                if owner == "Self" || owner.chars().next().is_some_and(char::is_uppercase) {
                    "static_call"
                } else {
                    "func_call"
                }
            }
            Callee::Path(_) => "func_call",
            Callee::Method { .. } => "method_call",
            Callee::Macro(_) => "macro",
        }
    }

    /// A kimenet `target` mezője (a leírt alak).
    pub(crate) fn target(&self) -> String {
        match self {
            Callee::Path(segs) => segs.join("::"),
            Callee::Method { name, .. } => name.clone(),
            Callee::Macro(segs) => format!("{}!", segs.join("::")),
        }
    }
}

/// A kategorizálás bemenete: a hívás, a hívó helye és a fájl névtáblája.
pub(crate) struct CallInput<'a> {
    pub callee: &'a Callee,
    /// A hívó containere (a `Self` és a `self.x()` feloldásához).
    pub container: &'a str,
    /// A beágyazott (inline) modulok útja a hívás helyén.
    pub inline: &'a [String],
    /// A hívott név egy helyi változó / paraméter / closure (`f(x)`).
    pub bound_locally: bool,
    pub resolver: &'a Resolver,
}

/// A kategorizálás eredménye.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Verdict {
    pub category: &'static str,
    pub resolved: Option<String>,
}

impl Verdict {
    fn new(category: &'static str, resolved: Option<String>) -> Self {
        Self { category, resolved }
    }
}

type CallRule = dyn for<'a> Rule<CallInput<'a>, Output = Verdict> + Send + Sync;

/// A szabálylánc (a sorrend a prioritás); tartalék: `external_call` (ismeretlen fogadójú metódus).
pub(crate) struct Categorizer {
    chain: Chain<CallRule, Verdict>,
}

impl Categorizer {
    pub(crate) fn new(rules: Vec<Box<CallRule>>) -> Self {
        Self {
            chain: Chain::new(rules, Verdict::new("external_call", None)),
        }
    }

    /// Az alapértelmezett lánc: makró → helyi változó → self → helyi elem → std → framework → crate → csomag.
    pub(crate) fn with_defaults() -> Self {
        Self::new(rules::default_rules())
    }

    pub(crate) fn categorize(&self, input: &CallInput<'_>) -> Verdict {
        let mut verdict = self.chain.resolve(input);
        if verdict.resolved.as_deref() == Some(input.callee.target().as_str()) {
            verdict.resolved = None;
        }
        verdict
    }
}

#[cfg(test)]
mod tests;
