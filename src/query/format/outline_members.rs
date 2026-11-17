//! Az `outline` konténer-tagjai: konstansok, Vue `emits` / `models`, tulajdonságok (props); a kompakt alakban
//! a hosszú listák egy tördelt sorba vonva.

use crate::model::{Constant, Container, FileGraph, Property};
use crate::query::QueryResult;
use std::fmt::Write as _;

/// A kompakt alak sor-szélessége (a tördelt listáké).
const WRAP: usize = 110;
/// Ennél hosszabb props-sorban az alapérték rövidítve.
const LONG_LINE: usize = 60;

/// A Vue-komponens `defineProps<X>()` típus-interfésze: a komponens props-listája már ugyanezeket mondja, ezért az
/// interfész nem külön konténer (a tulajdonság-halmazuk azonos, a komponens is a fájlban van).
pub(super) fn is_props_duplicate(g: &FileGraph, c: &Container) -> bool {
    if c.kind != "interface" {
        return false;
    }
    let names = |container: &Container| -> Vec<&str> {
        let mut names: Vec<&str> = g
            .properties
            .iter()
            .filter(|p| g.owns(container, p.container.as_deref(), p.line))
            .map(|p| p.name.trim_start_matches('$'))
            .collect();
        names.sort_unstable();
        names
    };
    let mine = names(c);
    !mine.is_empty()
        && g.containers
            .iter()
            .any(|other| other.kind == "component" && names(other) == mine)
}

/// A lista tördelve: `fejléc` + elemek `, `-vel, `WRAP` oszlopnál új (behúzott) sor.
fn wrapped(s: &mut String, head: &str, items: &[String]) {
    let mut line = head.to_owned();
    let mut first = true;
    for (i, item) in items.iter().enumerate() {
        let sep = if first { "" } else { " " };
        let last = i + 1 == items.len();
        let piece = format!("{sep}{item}{}", if last { "" } else { "," });
        if !first && line.chars().count() + piece.chars().count() > WRAP {
            s.push_str(&line);
            s.push('\n');
            line = format!("    {item}{}", if last { "" } else { "," });
        } else {
            line.push_str(&piece);
        }
        first = false;
    }
    s.push_str(&line);
    s.push('\n');
}

/// A konténer konstansai: az `emit:` / `model:` konstansok egy-egy `emits (N): a, b` / `models (N): …` sorban, a
/// többi `constants (N):` alatt soronként (`compact`: tördelt egy-két sorban).
pub(super) fn constants(
    s: &mut String,
    row: &QueryResult,
    c: &Container,
    locks: Option<&std::collections::HashMap<String, String>>,
    compact: bool,
) {
    let g = &row.graph;
    let all: Vec<&Constant> = g
        .constants
        .iter()
        .filter(|x| g.owns(c, x.container.as_deref(), x.line))
        .collect();
    let of_kind =
        |kind: &str| -> Vec<&Constant> { all.iter().copied().filter(|x| x.kind == kind).collect() };
    let emits = of_kind("emit");
    let models = of_kind("model");
    let rest: Vec<&Constant> = all
        .iter()
        .copied()
        .filter(|x| x.kind != "emit" && x.kind != "model")
        .collect();
    if !rest.is_empty() {
        let items: Vec<String> = rest
            .iter()
            .map(|x| {
                let value = x
                    .value
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                let case = if x.kind == "enum_case" {
                    " [enum case]"
                } else {
                    ""
                };
                let locked = locks
                    .and_then(|locks| super::outline::locked_version(locks, x))
                    .map(|version| format!(" · lock {version}"))
                    .unwrap_or_default();
                format!("{}{value}{case}{locked}", x.name)
            })
            .collect();
        let head = format!("  constants ({}):", items.len());
        if compact && items.len() > 4 {
            wrapped(s, &format!("{head} "), &items);
        } else {
            writeln!(s, "{head}").unwrap();
            for item in items {
                writeln!(s, "    {item}").unwrap();
            }
        }
    }
    if !emits.is_empty() {
        let names: Vec<&str> = emits
            .iter()
            .map(|x| x.name.strip_prefix("emit:").unwrap_or(&x.name))
            .collect();
        writeln!(s, "  emits ({}): {}", names.len(), names.join(", ")).unwrap();
    }
    if !models.is_empty() {
        let items: Vec<String> = models
            .iter()
            .map(|x| {
                let name = x.name.strip_prefix("model:").unwrap_or(&x.name);
                match &x.value {
                    Some(value) => format!("{name}: {value}"),
                    None => name.to_owned(),
                }
            })
            .collect();
        writeln!(s, "  models ({}): {}", items.len(), items.join(", ")).unwrap();
    }
}

/// Egy tulajdonság sora (behúzás nélkül): `típus $név = alapérték [readonly] [static]`; ha a sor `LONG_LINE`-nál
/// hosszabb, az alapérték rövidítve (`…`).
fn property_text(p: &Property) -> String {
    let flags = format!(
        "{}{}",
        if p.readonly { " [readonly]" } else { "" },
        if p.is_static { " [static]" } else { "" }
    );
    let base = format!(
        "{} ${}",
        p.type_hint.clone().unwrap_or_else(|| "mixed".to_owned()),
        p.name.trim_start_matches('$')
    );
    let Some(default) = p.default.as_deref() else {
        return format!("{base}{flags}");
    };
    let full = format!("{base} = {default}{flags}");
    if full.chars().count() <= LONG_LINE {
        return full;
    }
    let one: String = default
        .chars()
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect();
    let room = LONG_LINE.saturating_sub(base.chars().count() + flags.chars().count() + 3);
    let cut: String = one.chars().take(room.max(12)).collect();
    format!("{base} = {cut}…{flags}")
}

/// A konténer tulajdonságai (`compact`: tördelt egy-két sorban).
pub(super) fn properties(s: &mut String, g: &FileGraph, c: &Container, compact: bool) {
    let items: Vec<String> = g
        .properties
        .iter()
        .filter(|p| g.owns(c, p.container.as_deref(), p.line))
        .map(property_text)
        .collect();
    if items.is_empty() {
        return;
    }
    let head = format!("  properties ({}):", items.len());
    if compact && items.len() > 4 {
        wrapped(s, &format!("{head} "), &items);
    } else {
        writeln!(s, "{head}").unwrap();
        for item in items {
            writeln!(s, "    {item}").unwrap();
        }
    }
}
