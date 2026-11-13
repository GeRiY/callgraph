//! pnpm: a `pnpm-lock.yaml` közvetlen függőségei, kézzel írt, behúzás-alapú sor-olvasóval (nem általános YAML).
//! v6+ / v9: `importers:` → `.` (a gyökér) → `dependencies:` / `devDependencies:` / `optionalDependencies:` →
//! `<név>:` → `version: <v>` (a `(peer…)` utótag nélkül); ha nincs `importers`, ugyanezek a szakaszok a legfelső
//! szinten (egyprojektes v6: `<név>:` + `version:`, régi formátum: `<név>: <verzió>`, a `_peer` utótag nélkül).

use super::{LockSource, installed};
use crate::packages::model::Installed;

/// A `pnpm-lock.yaml` stratégiája.
pub(crate) struct PnpmLock;

/// A függőség-szakaszok a gyökér alatt.
const SECTIONS: [&str; 3] = ["dependencies", "devDependencies", "optionalDependencies"];

impl LockSource for PnpmLock {
    fn file(&self) -> &'static str {
        "pnpm-lock.yaml"
    }

    fn read(&self, text: &str) -> Vec<Installed> {
        let lines: Vec<Line<'_>> = text.lines().filter_map(parse_line).collect();
        let root: &[Line<'_>] = match child(&lines, "importers").and_then(|all| child(all, ".")) {
            Some(root) => root,
            None => &lines,
        };
        let mut out = Vec::new();
        for section in SECTIONS {
            if let Some(block) = child(root, section) {
                deps_into(block, &mut out);
            }
        }
        out
    }
}

/// Egy `kulcs: érték` / `kulcs:` sor a behúzásával; a kulcs és az érték idézőjelek nélkül.
struct Line<'a> {
    indent: usize,
    key: &'a str,
    value: &'a str,
}

/// Egy sor; az üres, a komment és a lista-elem (`- …`) sor kimarad.
fn parse_line(raw: &str) -> Option<Line<'_>> {
    let raw = raw.trim_end();
    let body = raw.trim_start();
    if body.is_empty() || body.starts_with('#') || body.starts_with('-') {
        return None;
    }
    let (key, value) = split_key(body)?;
    Some(Line {
        indent: raw.len() - body.len(),
        key: unquote(key),
        value: unquote(value.trim()),
    })
}

/// `'@vue/shared': x` / `vue: x` / `vue:` → (kulcs, érték).
fn split_key(body: &str) -> Option<(&str, &str)> {
    if let Some(quote) = body.chars().next().filter(|c| *c == '\'' || *c == '"') {
        let close = body[1..].find(quote)? + 1;
        let rest = body[close + 1..].strip_prefix(':')?;
        return Some((&body[..=close], rest));
    }
    if let Some(at) = body.find(": ") {
        return Some((&body[..at], &body[at + 2..]));
    }
    body.strip_suffix(':').map(|key| (key, ""))
}

fn unquote(text: &str) -> &str {
    for quote in ['\'', '"'] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    text
}

/// A szelet legkisebb behúzású (közvetlen) sorai közül a `key` nevű alatti blokk.
fn child<'s, 'a>(lines: &'s [Line<'a>], key: &str) -> Option<&'s [Line<'a>]> {
    let level = lines.iter().map(|line| line.indent).min()?;
    let at = lines
        .iter()
        .position(|line| line.indent == level && line.key == key)?;
    Some(block_after(lines, at))
}

/// Az `at` sor alatti, nála mélyebben behúzott sorok.
fn block_after<'s, 'a>(lines: &'s [Line<'a>], at: usize) -> &'s [Line<'a>] {
    let level = lines[at].indent;
    let rest = &lines[at + 1..];
    let end = rest
        .iter()
        .position(|line| line.indent <= level)
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Egy függőség-szakasz közvetlen sorai: `<név>: <verzió>` (régi), vagy `<név>:` + alatta `version: <verzió>`.
fn deps_into(section: &[Line<'_>], out: &mut Vec<Installed>) {
    let Some(level) = section.iter().map(|line| line.indent).min() else {
        return;
    };
    for (at, line) in section.iter().enumerate() {
        if line.indent != level {
            continue;
        }
        let raw = if line.value.is_empty() {
            match block_after(section, at)
                .iter()
                .find(|inner| inner.key == "version")
            {
                Some(inner) => inner.value,
                None => continue,
            }
        } else {
            line.value
        };
        let version = clean(raw);
        if version.is_empty() || version.contains(':') {
            continue;
        }
        out.push(installed(line.key, version));
    }
}

/// `3.4.21(typescript@5.3.3)` / `4.0.0_@types+node@18` → `3.4.21` / `4.0.0`.
fn clean(version: &str) -> &str {
    version.split(['(', '_']).next().unwrap_or(version).trim()
}
