//! A Blade-sablon PHP-kifejezései (`{{ $order->total() }}`, `@if ($user->isAdmin())`, `@php … @endphp`,
//! `:attr="…"`) a meglévő PHP-elemzővel: a kifejezések egy szintetikus PHP-forrásba kerülnek úgy, hogy mindegyik
//! pontosan a sablonbeli sorára esik (a sorok közti üres sorokkal), így a hívások sora a sablon sora. Ha az együtt
//! elemzett forrás szintaktikai hibás (egy rossz kifejezés), a kifejezések külön-külön futnak, és a hibásak kimaradnak.

use crate::lang::php::PhpBackend;
use crate::model::Call;

/// Egy PHP-kifejezés (vagy utasítás-sor) a sablonban.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Chunk {
    /// A sablon sora, ahol a kifejezés kezdődik.
    pub line: u32,
    pub code: String,
}

/// A szintetikus PHP-forrás: minden kifejezés a saját sorában, `;`-vel lezárva, egy osztály egy metódusában.
fn source(chunks: &[&Chunk]) -> String {
    let mut src = String::from("<?php class __Blade { function __view() {");
    let mut line = 1u32;
    for chunk in chunks {
        while line < chunk.line {
            src.push('\n');
            line += 1;
        }
        src.push(' ');
        src.push_str(&chunk.code);
        let trimmed = chunk.code.trim_end();
        if !(trimmed.ends_with(';') || trimmed.ends_with('}')) {
            src.push(';');
        }
        line += u32::try_from(chunk.code.matches('\n').count()).unwrap_or(0);
    }
    src.push_str("\n} }\n");
    src
}

/// A kifejezések hívásai (a sablon soraival). A hívás `container` / `caller` mezője a szintetikus osztály / metódus:
/// az elemző cseréli le a sablon szerkezetére.
pub(super) fn calls(chunks: &[Chunk]) -> Vec<Call> {
    if chunks.is_empty() {
        return Vec::new();
    }
    let backend = PhpBackend::with_defaults();
    let all: Vec<&Chunk> = chunks.iter().collect();
    let analysis = backend.analyze_source("blade-expressions.php", source(&all).as_bytes());
    if analysis.graph.parse_errors.is_empty() {
        return analysis.graph.calls;
    }
    chunks
        .iter()
        .flat_map(|chunk| {
            let one = backend.analyze_source("blade-expression.php", source(&[chunk]).as_bytes());
            if one.graph.parse_errors.is_empty() {
                one.graph.calls
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// Sima string-literál argumentum értéke a hívás tárolt argumentumából (`'users.store'`); más: `None`.
pub(super) fn literal_arg(call: &Call, index: usize) -> Option<String> {
    super::scan::literal(call.args.as_ref()?.get(index)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(line: u32, code: &str) -> Chunk {
        Chunk {
            line,
            code: code.to_owned(),
        }
    }

    #[test]
    fn calls_keep_the_template_lines() {
        let found = calls(&[
            chunk(3, "$order->total()"),
            chunk(7, "Str::limit($post->body, 20)"),
            chunk(9, "route('users.store')"),
        ]);
        let at = |target: &str| {
            found
                .iter()
                .find(|c| c.target.ends_with(target))
                .map(|c| c.line)
        };
        assert_eq!(at("total"), Some(3));
        assert_eq!(at("limit"), Some(7));
        assert_eq!(at("route"), Some(9));
        let route = found.iter().find(|c| c.target == "route").unwrap();
        assert_eq!(literal_arg(route, 0).as_deref(), Some("users.store"));
    }

    #[test]
    fn a_broken_expression_only_loses_itself() {
        let found = calls(&[
            chunk(2, "$a->ok()"),
            chunk(4, "$b->("),
            chunk(6, "$c->fine()"),
        ]);
        let targets: Vec<&str> = found.iter().map(|c| c.target.as_str()).collect();
        assert!(
            targets.contains(&"ok") && targets.contains(&"fine"),
            "{targets:?}"
        );
        assert!(found.iter().all(|c| c.line != 4));
    }
}
