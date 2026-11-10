//! A javaslat-szabályok: determinisztikusan, csak a tényleges kimenetben szó szerint szereplő nevekből és utakból.
//! Minden szabály kicsi, és csak egyértelmű jelöltnél ad javaslatot (különben semmit); legfeljebb 3 javaslat.
//! Csak olvasó navigáció: outline, `--body`, név szerinti hívók, `--depth 2`, `--by-layer`, `--layer X`, a levágott
//! kimenet szűkítése, `profile --full`.

use std::collections::BTreeSet;
use std::path::Path;

use regex::Regex;

use super::command::{absolute, body_of, callers_of, outline_of, relative, show_args};
use super::invocation::{Invocation, ShowCall};
use super::output::{method_line, method_lines, method_name};
use super::suggestion::{Suggestion, SuggestionKind};

/// Ennyi javaslat fér egy sorba.
pub(crate) const MAX_SUGGESTIONS: usize = 3;

/// A locate „pontos metódus / függvény” címkéi mindkét nyelven.
const EXACT_LABELS: [&str; 4] = [
    "(exact method)",
    "(exact function)",
    "(pontos metódus)",
    "(pontos függvény)",
];

/// A javaslatok egy futás kimenetéből. `layer_of`: a gyökérhez relatív út rétege (a `--by-layer` szabályhoz).
pub(crate) fn suggest(
    invocation: &Invocation,
    root: &Path,
    output: &str,
    layer_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<Suggestion> {
    let mut found = match invocation {
        Invocation::Locate { .. } => locate(root, output),
        Invocation::Profile { full: false, .. } => profile(root, output),
        Invocation::Profile { full: true, .. } => Vec::new(),
        Invocation::Show(call) if call.machine() || call.other || !call.plain_format() => {
            Vec::new()
        }
        Invocation::Show(call) if call.grep.is_some() => callers(call, root, output, layer_of),
        Invocation::Show(call) => outline(call, root, output),
    };
    let mut seen = BTreeSet::new();
    found.retain(|s| seen.insert(s.args.clone()));
    found.truncate(MAX_SUGGESTIONS);
    found
}

/// `locate`: az 1. fájl outline-ja; ha az indoklása pontos metódus / függvény, annak törzse és hívói.
fn locate(root: &Path, output: &str) -> Vec<Suggestion> {
    let mut lines = output.lines().skip_while(|l| !l.starts_with("1. "));
    let Some(first) = lines.next() else {
        return Vec::new();
    };
    let Some(path) = first["1. ".len()..]
        .split("  ")
        .next()
        .filter(|p| !p.is_empty())
    else {
        return Vec::new();
    };
    let mut found = vec![outline_of(root, path, SuggestionKind::Outline)];
    let exact = lines
        .take_while(|l| l.starts_with("   "))
        .map(str::trim)
        .find(|l| EXACT_LABELS.iter().any(|label| l.ends_with(label)))
        .and_then(method_name);
    if let Some(name) = exact {
        found.push(body_of(root, path, &name, SuggestionKind::Body));
        found.push(callers_of(root, &name));
    }
    found
}

/// Egyfájlos outline (vagy a levágott kimenet metódus-indexe): a leghosszabb metódus törzse, ha legalább 3 metódus
/// van és a leghosszabb neve egyedi; egy levágott mappa-summary: a legtöbb metódusú fájl outline-ja.
fn outline(call: &ShowCall, root: &Path, output: &str) -> Vec<Suggestion> {
    if call.body || call.depth.is_some() || call.by_layer || call.targets.len() != 1 {
        return Vec::new();
    }
    if call.targets[0].contains("::") {
        return Vec::new();
    }
    let truncated = crate::stats::truncated(output);
    let files: Vec<&str> = output
        .lines()
        .filter_map(|l| crate::notes::strip_mark(l).strip_prefix("FILE  "))
        .collect();
    if files.len() == 1 {
        let methods = method_lines(output);
        if methods.len() < 3 {
            return Vec::new();
        }
        let Some(longest) = methods.iter().max_by_key(|m| m.end.saturating_sub(m.start)) else {
            return Vec::new();
        };
        let span = longest.end.saturating_sub(longest.start);
        let ties = methods
            .iter()
            .filter(|m| m.name == longest.name || m.end.saturating_sub(m.start) == span)
            .count();
        if ties != 1 {
            return Vec::new();
        }
        let kind = if truncated {
            SuggestionKind::Narrow
        } else {
            SuggestionKind::Body
        };
        return vec![body_of(root, files[0].trim(), &longest.name, kind)];
    }
    if truncated && files.is_empty() {
        return summary_file(root, &call.targets[0], output)
            .map(|path| vec![outline_of(root, &path, SuggestionKind::Narrow)])
            .unwrap_or_default();
    }
    Vec::new()
}

/// A levágott mappa-summary legtöbb metódusú fájlja (holtversenyben az első), ha létező fájl.
fn summary_file(root: &Path, target: &str, output: &str) -> Option<String> {
    let re = Regex::new(r"^(\S+)  .*\((\d+) (?:methods?|metódus)\)").ok()?;
    let mut totals: Vec<(String, u64)> = Vec::new();
    for caps in output.lines().filter_map(|l| re.captures(l)) {
        let n: u64 = caps[2].parse().unwrap_or(0);
        match totals.iter_mut().find(|(p, _)| *p == caps[1]) {
            Some((_, total)) => *total += n,
            None => totals.push((caps[1].to_owned(), n)),
        }
    }
    let best = totals.iter().map(|(_, n)| *n).max().filter(|n| *n > 0)?;
    let (path, _) = totals.iter().find(|(_, n)| *n == best)?;
    let dir = absolute(root, target);
    [Path::new(&dir).join(path), root.join(path)]
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
}

/// Név szerinti hívó-keresés: `--depth 2` ugyanarra, az első hívó törzse, és `--by-layer`, ha a hívók legalább
/// két réteghez tartoznak.
fn callers(
    call: &ShowCall,
    root: &Path,
    output: &str,
    layer_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<Suggestion> {
    if call.body || call.depth.as_deref().is_some_and(|d| d != "1") {
        return Vec::new();
    }
    let Some(pattern) = call.grep.as_deref() else {
        return Vec::new();
    };
    let base = output
        .lines()
        .next()
        .and_then(|l| {
            l.strip_prefix("root: ")
                .or_else(|| l.strip_prefix("gyökér: "))
        })
        .map_or_else(|| root.to_path_buf(), |r| Path::new(r.trim()).to_path_buf());
    let mut first: Option<(String, String)> = None;
    let mut files: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in output.lines() {
        // a sor végi jegyzet-jelölő (`  📝 N`) nem része az útnak
        if let Some(file) = crate::notes::strip_mark(line).strip_prefix("FILE  ") {
            current = Some(file.trim().to_owned());
            files.push(file.trim().to_owned());
        } else if let (Some(file), None) = (&current, &first)
            && let Some(method) = method_line(line)
        {
            first = Some((file.clone(), method.name));
        }
    }
    let Some((file, name)) = first else {
        return Vec::new();
    };
    let targets: Vec<String> = call.targets.iter().map(|t| absolute(root, t)).collect();
    let mut flags = vec!["--grep".to_owned(), pattern.to_owned()];
    if let Some(layer) = &call.layer {
        flags.extend(["--layer".to_owned(), layer.clone()]);
    }
    let mut deeper = flags.clone();
    deeper.extend(["--depth".to_owned(), "2".to_owned()]);
    let mut found = vec![Suggestion {
        kind: SuggestionKind::Depth,
        args: show_args(root, &deeper, &targets),
        label: crate::t!("--depth 2 (same search)", "--depth 2 (ugyanez a keresés)").to_owned(),
    }];
    let file_abs = absolute(&base, &file);
    found.push(Suggestion {
        kind: SuggestionKind::Body,
        args: show_args(
            root,
            &["--body".to_owned()],
            &[format!("{file_abs}::{name}")],
        ),
        label: format!("{}::{name} --body", relative(root, &file_abs)),
    });
    let layers: BTreeSet<String> = files
        .iter()
        .filter_map(|f| layer_of(&relative(root, &absolute(&base, f))))
        .collect();
    if !call.by_layer && layers.len() >= 2 {
        let mut grouped = flags;
        grouped.push("--by-layer".to_owned());
        found.push(Suggestion {
            kind: SuggestionKind::ByLayer,
            args: show_args(root, &grouped, &targets),
            label: crate::t!("--by-layer (same search)", "--by-layer (ugyanez a keresés)")
                .to_owned(),
        });
    }
    found
}

/// A rövid profil: a legnagyobb réteg summary-ja a gyökéren, és `profile --full`, ha a rövid nézet rétegeket rejtett el.
fn profile(root: &Path, output: &str) -> Vec<Suggestion> {
    let mut found = Vec::new();
    let mut best: Option<(String, u64)> = None;
    let mut hidden = false;
    let rows = output
        .lines()
        .skip_while(|l| *l != "layers (files):" && *l != "rétegek (fájlok):")
        .skip(1)
        .take_while(|l| l.starts_with("  "));
    for row in rows {
        if row.trim_start().starts_with('…') {
            hidden = row.contains("(profile --full)");
            continue;
        }
        let fields: Vec<&str> = row
            .split("  ")
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .collect();
        let Some(label) = fields.first() else {
            continue;
        };
        let Some(count) = fields.iter().skip(1).find_map(|f| f.parse::<u64>().ok()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(_, n)| count > *n) {
            best = Some((layer_flag(label), count));
        }
    }
    if let Some((layer, _)) = best {
        let root_text = root.display().to_string();
        found.push(Suggestion {
            kind: SuggestionKind::LayerSummary,
            args: show_args(root, &["--layer".to_owned(), layer.clone()], &[root_text]),
            label: crate::tf!("--layer {layer} summary", "--layer {layer} summary"),
        });
    }
    if hidden {
        found.push(Suggestion {
            kind: SuggestionKind::ProfileFull,
            args: vec![
                "profile".to_owned(),
                "--project".to_owned(),
                root.display().to_string(),
                "--full".to_owned(),
            ],
            label: "profile --full".to_owned(),
        });
    }
    found
}

/// `view (rust)` → `view:rust` (a `--layer` mindkét alakot érti; ez héjban idézés nélkül is jó).
fn layer_flag(label: &str) -> String {
    match label.strip_suffix(')').and_then(|l| l.split_once(" (")) {
        Some((role, eco)) => format!("{role}:{eco}"),
        None => label.to_owned(),
    }
}
