//! A stílus-emitter: `selector{decl;decl}` szabályokból behúzott CSS-sorok (2 szóköz, szabályok között üres sor).

use super::model::Rule;
use crate::generate::head::{split_once_top, split_top};

const IND: &str = "  ";

/// A szabályok sorai.
pub fn lines(rules: &[Rule]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, rule) in rules.iter().enumerate() {
        if i > 0 {
            out.push(String::new());
        }
        out.push(format!("{} {{", rule.selector));
        out.extend(body(&rule.body));
        out.push("}".to_owned());
    }
    out
}

/// A törzs sorai: beágyazott blokknál (`{` a törzsben) a kézzel tördelt sorok, különben `prop: value;` deklarációk.
fn body(raw: &str) -> Vec<String> {
    if raw.contains('{') {
        return raw
            .lines()
            .map(|l| {
                if l.trim().is_empty() {
                    String::new()
                } else {
                    format!("{IND}{l}")
                }
            })
            .collect();
    }
    split_top(raw, ';')
        .iter()
        .map(|d| d.lines().map(str::trim).collect::<Vec<_>>().join(" "))
        .filter(|d| !d.is_empty())
        .map(|d| match split_once_top(&d, ':') {
            Some((prop, value)) => format!("{IND}{}: {};", prop.trim(), value.trim()),
            None => format!("{IND}{d};"),
        })
        .collect()
}
