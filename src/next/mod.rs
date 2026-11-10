//! Javasolt következő lépések (1. fázis: szabály-alapú, opt-in). Egy eredmény a végén egy rövid azonosítót és
//! legfeljebb 3 számozott, csak olvasó következő parancsot kaphat; az agent az azonosítóval futtatja
//! (`callgraph.sh next r7 1` / `next r7 1,3`; MCP: `next` eszköz) ahelyett, hogy maga rakná össze.
//!
//! Biztosítékok (a tulajdonos kemény követelménye: soha ne vigye rossz irányba az agentet):
//! - csak navigáció (outline, `--body`, hívók, `--depth 2`, `--by-layer`, `--layer X`, szűkítés, `profile --full`);
//! - a jelöltek determinisztikus szabályokból, csak a kimenetben szó szerint szereplő nevekből / utakból (`rules`);
//!   egy későbbi rangsoroló csak sorba rendezhet / szűrhet (`Suggestion`, `SuggestionKind`);
//! - sor csak egyértelmű jelöltnél; hibánál, üres eredménynél, gépi formátumnál soha;
//! - semleges szöveg, egy sor a legvégén;
//! - alapból KI: `CALLGRAPH_NEXT=1` vagy a globális `--next` kapcsoló (CLI és `callgraph mcp --next`).

mod command;
mod invocation;
mod log;
mod output;
mod rules;
mod run;
mod store;
mod suggestion;
#[cfg(test)]
mod tests;

pub use run::decorate;
pub(crate) use run::{annotate, parse_choices, parse_id, run_choices};
pub use suggestion::{Suggestion, SuggestionKind};

/// A globális kapcsoló.
pub const NEXT_FLAG: &str = "--next";
/// A környezeti változó (`1` / `true` / `yes` / `on`: be); a belépő a `--next` kapcsolóra is ezt állítja be.
pub const NEXT_ENV: &str = "CALLGRAPH_NEXT";

/// A globális `--next` kapcsoló kivétele az argumentumok közül (az első `--` előtt); `true`, ha szerepelt.
pub fn take_next(args: &mut Vec<String>) -> bool {
    let end = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let before = args.len();
    let mut at = 0;
    args.retain(|a| {
        at += 1;
        at > end || a != NEXT_FLAG
    });
    args.len() != before
}

/// Be vannak-e kapcsolva a javaslatok.
pub(crate) fn enabled() -> bool {
    std::env::var(NEXT_ENV).is_ok_and(|v| is_on(&v))
}

fn is_on(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}
