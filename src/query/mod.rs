//! Lekérdező-réteg: gráfok kiválasztása, formázása és kimeneti korlátozása.
//!
//! A kimenet szövegei kétnyelvűek: a formázók explicit `Lang`-ot kapnak (a `FormatterRegistry` és az `OutputLimit`
//! alapból a futás nyelvét, `crate::i18n::lang()`), így mindkét nyelv tesztelhető környezeti változó nélkül.

/// Formázott szöveg két nyelven, explicit nyelvvel: `ltf!(lang, "{n} calls", "{n} hívás")` → `String`
/// (a `tf!` párja a formázóknak, amelyek a nyelvet paraméterként kapják).
macro_rules! ltf {
    ($lang:expr, $en:literal, $hu:literal $(, $arg:expr)* $(,)?) => {
        match $lang {
            $crate::i18n::Lang::En => format!($en $(, $arg)*),
            $crate::i18n::Lang::Hu => format!($hu $(, $arg)*),
        }
    };
}

mod engine;
mod filter;
pub mod format;
mod limit;
mod selector;
pub(crate) mod snippet;

pub use engine::{QueryEngine, QueryResult, ViaCall};
pub use limit::OutputLimit;
pub use selector::{Selector, SelectorSet};
