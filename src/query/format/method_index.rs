//! `method-index` formázó: a kimenet-korlát egyfájlos tartaléknézete.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct MethodIndex;
impl Formatter for MethodIndex {
    fn format(&self, rows: &[QueryResult], lang: crate::i18n::Lang) -> String {
        let mut s = String::new();
        for r in rows {
            writeln!(
                s,
                "FILE  {}{}",
                r.graph.file,
                crate::io::protect_mark(&r.graph.file)
            )
            .unwrap();
            for c in &r.graph.containers {
                writeln!(s, "{} {}  L{}-{}", c.kind, c.name, c.line, c.end_line).unwrap();
                let mut methods: Vec<_> = r
                    .graph
                    .methods
                    .iter()
                    .filter(|m| r.graph.owns(c, m.container.as_deref(), m.line))
                    .collect();
                methods.sort_by_key(|m| m.line);
                if methods.is_empty() && c.kind == "document" {
                    key_index(&mut s, r, c, lang);
                }
                for m in methods {
                    // a Vue-komponens `<script setup>` tagjai mind privátak: a címke nem információ
                    let visibility = match m.visibility.as_deref() {
                        Some("private") if r.graph.sfc.is_some() => String::new(),
                        other => format!("[{}]  ", other.unwrap_or("public")),
                    };
                    writeln!(
                        s,
                        "  {}()  L{}-{}  {visibility}{} {}",
                        m.name,
                        m.line,
                        m.end_line.unwrap_or(m.line),
                        lang.pick("calls:", "hívás:"),
                        r.graph
                            .calls
                            .iter()
                            .filter(|x| x.container == m.container
                                && x.caller.as_deref() == Some(&m.name))
                            .count()
                    )
                    .unwrap();
                }
            }
        }
        s
    }
}

/// Egy kulcs-index legfeljebb ennyi sort kap (a többi egy `… +N` sorba kerül).
const MAX_KEY_LINES: usize = 24;
/// Ennyi egymás utáni azonos típusú skalár kulcstól vonja össze egy sorba a kulcs-index.
const MIN_RUN: usize = 5;

/// A tartalék nézet metódus nélküli dokumentumra (JSON): a felső szintű kulcsok a gyermekszámmal; az azonos típusú
/// skalár kulcsok sorozata egy sorba vonva (`key_0 … key_449  450 string  L5-454`), a lista `MAX_KEY_LINES` sorra vágva.
fn key_index(
    s: &mut String,
    r: &QueryResult,
    c: &crate::model::Container,
    lang: crate::i18n::Lang,
) {
    let keys: Vec<_> = r
        .graph
        .constants
        .iter()
        .filter(|k| k.kind == "key" && r.graph.owns(c, k.container.as_deref(), k.line))
        .collect();
    let compound = |value: Option<&str>| {
        value.is_some_and(|v| v.starts_with("object (") || v.starts_with("array ("))
    };
    // (kulcs, leszármazottak száma, az utolsó leszármazott sora)
    let mut tops: Vec<(&crate::model::Constant, usize, u32)> = Vec::new();
    for k in keys {
        if let Some((top, n, last)) = tops.last_mut()
            && compound(top.value.as_deref())
            && k.name.starts_with(&format!("{}.", top.name))
        {
            *n += 1;
            *last = k.line;
        } else {
            tops.push((k, 0, k.line));
        }
    }
    let kind_of = |value: Option<&str>| -> &'static str {
        match value.and_then(|v| v.chars().next()) {
            Some('"') => "string",
            Some(ch) if ch.is_ascii_digit() || ch == '-' => "number",
            Some('t' | 'f') => "bool",
            Some('o' | 'a') => "compound",
            _ => "null",
        }
    };
    let mut lines: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tops.len() {
        let (key, n, last) = tops[i];
        let kind = kind_of(key.value.as_deref());
        let mut j = i + 1;
        if kind != "compound" {
            while j < tops.len() && kind_of(tops[j].0.value.as_deref()) == kind {
                j += 1;
            }
        }
        if j - i >= MIN_RUN {
            lines.push(format!(
                "  {} … {}  {} {kind}  L{}-{}",
                key.name,
                tops[j - 1].0.name,
                j - i,
                key.line,
                tops[j - 1].0.line
            ));
            i = j;
            continue;
        }
        let value = key.value.as_deref().unwrap_or_default();
        lines.push(if n > 0 {
            format!(
                "  {}  {value}  L{}-{last}  {} {}",
                key.name,
                key.line,
                n + 1,
                lang.pick("keys", "kulcs")
            )
        } else {
            format!("  {}  {value}  L{}", key.name, key.line)
        });
        i += 1;
    }
    if lines.len() > MAX_KEY_LINES {
        let rest = lines.len() - MAX_KEY_LINES;
        lines.truncate(MAX_KEY_LINES);
        lines.push(format!(
            "  … +{rest} {}",
            lang.pick("more entries", "további sor")
        ));
    }
    for line in lines {
        s.push_str(&line);
        s.push('\n');
    }
}
