//! Egy fájl indoklás-sorai: az egyezések jelölt (szimbólum) szerint összevonva — ugyanaz a string vagy hívás több
//! soron egy sor —, utána legfeljebb `limit` darab, előnyben azok, amelyek még nem szereplő kifejezést fednek le.

use super::matcher::{Hit, Kind, Symbol};
use super::terms::Term;

/// Egy indoklás-sor: a legerősebb fajta, az első jelölt, a lefedett kifejezések és a sorok.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reason {
    pub kind: Kind,
    pub symbol: Symbol,
    pub terms: Vec<usize>,
    pub lines: Vec<u32>,
}

/// A jelölt összevonási kulcsa: string és hívás érték / cél szerint (sorok összevonva), a többi helye szerint.
fn key(symbol: &Symbol) -> String {
    match symbol {
        Symbol::File => "file".into(),
        Symbol::Container { name, line, .. } => format!("c\0{name}\0{line}"),
        Symbol::Method {
            container,
            name,
            line,
            ..
        } => format!("m\0{}\0{name}\0{line}", container.as_deref().unwrap_or("")),
        Symbol::Constant { name, line } => format!("k\0{name}\0{line}"),
        Symbol::Property {
            container,
            name,
            line,
        } => format!("p\0{}\0{name}\0{line}", container.as_deref().unwrap_or("")),
        Symbol::Key { name, line } => format!("j\0{name}\0{line}"),
        Symbol::Str { value, .. } => format!("s\0{value}"),
        Symbol::Call { target, .. } => format!("x\0{target}"),
        Symbol::Package { name, .. } => format!("g\0{name}"),
    }
}

fn line_of(symbol: &Symbol) -> u32 {
    match symbol {
        Symbol::File => 0,
        Symbol::Container { line, .. }
        | Symbol::Method { line, .. }
        | Symbol::Constant { line, .. }
        | Symbol::Property { line, .. }
        | Symbol::Key { line, .. }
        | Symbol::Str { line, .. }
        | Symbol::Call { line, .. }
        | Symbol::Package { line, .. } => *line,
    }
}

/// Az egyezések összevonva, fajta és sor szerint rendezve.
pub(crate) fn group(hits: &[Hit]) -> Vec<Reason> {
    let mut keys: Vec<String> = Vec::new();
    let mut out: Vec<Reason> = Vec::new();
    for hit in hits {
        let k = key(&hit.symbol);
        let line = line_of(&hit.symbol);
        match keys.iter().position(|existing| *existing == k) {
            Some(at) => {
                let reason = &mut out[at];
                reason.kind = reason.kind.min(hit.kind);
                if !reason.terms.contains(&hit.term) {
                    reason.terms.push(hit.term);
                }
                if line > 0 && !reason.lines.contains(&line) {
                    reason.lines.push(line);
                }
            }
            None => {
                keys.push(k);
                out.push(Reason {
                    kind: hit.kind,
                    symbol: hit.symbol.clone(),
                    terms: vec![hit.term],
                    lines: if line > 0 { vec![line] } else { Vec::new() },
                });
            }
        }
    }
    for reason in &mut out {
        reason.terms.sort_unstable();
        reason.lines.sort_unstable();
    }
    out.sort_by_key(|reason| (reason.kind, reason.lines.first().copied().unwrap_or(0)));
    out
}

/// Legfeljebb `limit` indoklás: előbb az új kifejezést lefedők (a rendezés sorrendjében), utána a többi.
pub(crate) fn pick(reasons: Vec<Reason>, limit: usize) -> Vec<Reason> {
    let mut covered: Vec<usize> = Vec::new();
    let mut chosen: Vec<usize> = Vec::new();
    for (at, reason) in reasons.iter().enumerate() {
        if chosen.len() < limit && reason.terms.iter().any(|t| !covered.contains(t)) {
            covered.extend(reason.terms.iter().copied());
            chosen.push(at);
        }
    }
    for at in 0..reasons.len() {
        if chosen.len() >= limit {
            break;
        }
        if !chosen.contains(&at) {
            chosen.push(at);
        }
    }
    chosen.sort_unstable();
    reasons
        .into_iter()
        .enumerate()
        .filter(|(at, _)| chosen.contains(at))
        .map(|(_, reason)| reason)
        .collect()
}

/// Egy indoklás-sor szövege (a behúzás nélkül).
pub(crate) fn describe(reason: &Reason, terms: &[Term]) -> String {
    let what = match &reason.symbol {
        Symbol::File => crate::t!("file name", "fájlnév").to_owned(),
        Symbol::Container {
            kind,
            name,
            line,
            end,
        } => format!("{kind} {name}  L{line}-{end}"),
        Symbol::Method {
            name,
            line,
            end,
            callers,
            ..
        } => {
            let mut text = format!("{name}()  L{line}-{end}");
            if callers.count > 0 {
                let more = if callers.count > callers.sample.len() {
                    ", …"
                } else {
                    ""
                };
                text.push_str(&crate::tf!(
                    "  callers: {} ({}{})",
                    "  hívók: {} ({}{})",
                    callers.count,
                    callers.sample.join(", "),
                    more
                ));
            }
            text
        }
        Symbol::Constant { name, line } => format!("const {name}  L{line}"),
        Symbol::Property { name, line, .. } => format!("property {name}  L{line}"),
        Symbol::Key { name, line } => format!("{name}  L{line}"),
        Symbol::Str { value, .. } => format!("'{}'  {}", clip(value, 60), lines(&reason.lines)),
        Symbol::Package { label, .. } => format!("pkg {label}  {}", lines(&reason.lines)),
        Symbol::Call { caller, target, .. } => {
            let at = lines(&reason.lines);
            match caller {
                Some(caller) => crate::tf!(
                    "{caller}()  {at}  calls {target}",
                    "{caller}()  {at}  hívja: {target}"
                ),
                None => crate::tf!("{at}  calls {target}", "{at}  hívja: {target}"),
            }
        }
    };
    let quoted: Vec<String> = reason
        .terms
        .iter()
        .filter_map(|&t| terms.get(t))
        .map(|term| format!("\"{}\"", term.raw))
        .collect();
    let fuzzy = reason.kind == Kind::NamePart && !matches!(reason.symbol, Symbol::Package { .. });
    let arrow = if fuzzy { "~" } else { "←" };
    format!("{what}  {arrow} {} ({})", quoted.join(", "), label(reason))
}

/// A fajta címkéje zárójelben (`exact class`, `name part`, `string` …).
fn label(reason: &Reason) -> String {
    if matches!(reason.symbol, Symbol::Package { .. }) {
        return crate::t!("package", "csomag").to_owned();
    }
    match reason.kind {
        // a `route::név` definíciója (`->name('név')` a `routes/*.php`-ban)
        Kind::Definition if matches!(reason.symbol, Symbol::Str { .. }) => {
            crate::t!("route definition", "útvonal-definíció").to_owned()
        }
        Kind::Definition => {
            let what = match &reason.symbol {
                Symbol::Container { kind, .. } => kind.clone(),
                Symbol::Method {
                    container: None, ..
                } => crate::t!("function", "függvény").to_owned(),
                Symbol::Method { .. } => crate::t!("method", "metódus").to_owned(),
                Symbol::Key { .. } => crate::t!("JSON key", "JSON-kulcs").to_owned(),
                Symbol::Property { .. } => crate::t!("property", "property").to_owned(),
                _ => crate::t!("constant", "konstans").to_owned(),
            };
            crate::tf!("exact {what}", "pontos {what}")
        }
        Kind::FileName => crate::t!("file name", "fájlnév").to_owned(),
        Kind::NamePart => crate::t!("name part", "név-részlet").to_owned(),
        Kind::StringLit => crate::t!("string", "string").to_owned(),
        Kind::Call => crate::t!("call", "hívás").to_owned(),
    }
}

/// Sorszámok: `L33, L57`, háromnál több után `+N`.
fn lines(lines: &[u32]) -> String {
    let mut text = lines
        .iter()
        .take(3)
        .map(|line| format!("L{line}"))
        .collect::<Vec<_>>()
        .join(", ");
    if lines.len() > 3 {
        text.push_str(&format!(" +{}", lines.len() - 3));
    }
    text
}

/// Hosszú string rövidítve (karakterhatáron), egy sorba.
fn clip(value: &str, max: usize) -> String {
    let flat = value.replace(['\n', '\r'], " ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut out: String = flat.chars().take(max).collect();
    out.push('…');
    out
}
