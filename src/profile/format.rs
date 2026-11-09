//! A profil szöveges kimenete: tömör (~15 sor, alapértelmezés) és részletes (`--full`: mappánként, bizonyosság
//! szerint, a hívásirány párjai). A szövegek a kapott nyelven (kétnyelvű, mint a többi kimenet).

use std::fmt::Write as _;

use crate::i18n::Lang;

use super::project::{LayerSummary, ProjectProfile};

/// A tömör nézet legfeljebb ennyi réteg-sort ír ki.
const COMPACT_LAYERS: usize = 9;
/// A mappa-oszlop legnagyobb szélessége.
const FOLDER_WIDTH: usize = 40;

/// Kerüljön-e a végére a használati tipp (a CLI-ben és a `project_profile` eszközben igen, az MCP-útmutatóban nem).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tip {
    Show,
    Hide,
}

/// A profil szövege.
pub(crate) fn render(profile: &ProjectProfile, lang: Lang, full: bool, tip: Tip) -> String {
    let mut out = String::new();
    if profile.files == 0 {
        out.push_str(lang.pick(
            "profile: no code files (PHP, JS / TS / Vue, Swift, Rust) under the root\n",
            "profil: nincs kódfájl (PHP, JS / TS / Vue, Swift, Rust) a gyökér alatt\n",
        ));
        return out;
    }
    header(&mut out, profile, lang);
    if full {
        details(&mut out, profile, lang);
    } else {
        compact(&mut out, profile, lang);
    }
    if tip == Tip::Show {
        out.push_str(lang.pick(
            "💡 one layer only: --layer service (MCP: layer); summary grouped by layer: --layers; callers by layer: --by-layer (MCP: by_layer); details: profile --full\n",
            "💡 csak egy réteg: --layer service (MCP: layer); rétegenként csoportosított summary: --layers; hívók rétegenként: --by-layer (MCP: by_layer); részletek: profile --full\n",
        ));
    }
    out
}

fn header(out: &mut String, profile: &ProjectProfile, lang: Lang) {
    let architecture = if profile.architectures.is_empty() {
        lang.pick("no recognised architecture", "nem felismert architektúra")
            .to_owned()
    } else {
        profile
            .architectures
            .iter()
            .map(|a| a.label)
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let confidence = profile.confidence().label(lang);
    let _ = match lang {
        Lang::En => writeln!(out, "profile: {architecture} · confidence: {confidence}"),
        Lang::Hu => writeln!(out, "profil: {architecture} · bizonyosság: {confidence}"),
    };
}

fn compact(out: &mut String, profile: &ProjectProfile, lang: Lang) {
    if !profile.layers.is_empty() {
        out.push_str(lang.pick("layers (files):\n", "rétegek (fájlok):\n"));
        let shown: Vec<&LayerSummary> = profile.layers.iter().take(COMPACT_LAYERS).collect();
        let role_width = shown.iter().map(|l| l.label.len()).max().unwrap_or(0);
        let folder_width = shown
            .iter()
            .map(|l| main_folder(l, lang).chars().count())
            .max()
            .unwrap_or(0)
            .min(FOLDER_WIDTH);
        for layer in &shown {
            let folder = main_folder(layer, lang);
            let mut line = format!(
                "  {:<role_width$}  {:<folder_width$}  {:>4}",
                layer.label,
                folder,
                layer.files
            );
            if let Some(label) = layer.confirm_label {
                let _ = write!(line, "  ({label}: {})", layer.confirmed);
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        let rest = profile.layers.len().saturating_sub(COMPACT_LAYERS);
        if rest > 0 {
            let _ = match lang {
                Lang::En => writeln!(out, "  … +{rest} more layers (profile --full)"),
                Lang::Hu => writeln!(out, "  … +{rest} további réteg (profile --full)"),
            };
        }
    }
    patterns_line(out, profile, lang);
    flow_line(out, profile, lang);
    if !profile.unknown.is_empty() {
        let items: Vec<String> = profile
            .unknown
            .iter()
            .take(3)
            .map(|(folder, n)| files_label(folder, *n, lang))
            .collect();
        let more = profile.unknown.len().saturating_sub(3);
        let more = match (more, lang) {
            (0, _) => String::new(),
            (n, Lang::En) => format!(", +{n} more folders"),
            (n, Lang::Hu) => format!(", +{n} további mappa"),
        };
        let _ = match lang {
            Lang::En => writeln!(out, "unknown: {}{more}", items.join(", ")),
            Lang::Hu => writeln!(out, "ismeretlen: {}{more}", items.join(", ")),
        };
    }
    skipped_lines(out, profile, lang);
}

/// `vendor: 312 files skipped (extensions/Facebook, vendors/phpoffice, …)` és ugyanígy a generált kód.
fn skipped_lines(out: &mut String, profile: &ProjectProfile, lang: Lang) {
    let kinds = [
        (&profile.vendor, "vendor", "vendor"),
        (&profile.generated, "generated", "generált"),
    ];
    for (folders, en, hu) in kinds {
        if folders.is_empty() {
            continue;
        }
        let n: usize = folders.iter().map(|(_, n)| n).sum();
        let mut list: Vec<&str> = folders.iter().take(3).map(|(f, _)| f.as_str()).collect();
        if folders.len() > 3 {
            list.push("…");
        }
        let list = list.join(", ");
        let _ = match lang {
            Lang::En if n == 1 => writeln!(out, "{en}: 1 file skipped ({list})"),
            Lang::En => writeln!(out, "{en}: {n} files skipped ({list})"),
            Lang::Hu => writeln!(out, "{hu}: {n} fájl kihagyva ({list})"),
        };
    }
}

/// A réteg fő mappája; ha több van: `(+N)`.
fn main_folder(layer: &LayerSummary, lang: Lang) -> String {
    let Some((first, _)) = layer.folders.first() else {
        return String::new();
    };
    match (layer.folders.len() - 1, lang) {
        (0, _) => first.clone(),
        (n, Lang::En) => format!("{first} (+{n} more)"),
        (n, Lang::Hu) => format!("{first} (+{n} további)"),
    }
}

/// `app/Support (12 files)`.
fn files_label(folder: &str, n: usize, lang: Lang) -> String {
    match lang {
        Lang::En if n == 1 => format!("{folder} (1 file)"),
        Lang::En => format!("{folder} ({n} files)"),
        Lang::Hu => format!("{folder} ({n} fájl)"),
    }
}

fn patterns_line(out: &mut String, profile: &ProjectProfile, lang: Lang) {
    if profile.patterns.is_empty() {
        return;
    }
    let items: Vec<String> = profile
        .patterns
        .iter()
        .map(|p| match p.examples.first() {
            Some(example) => format!("{} ({}: {example})", p.name, p.files),
            None => format!("{} ({})", p.name, p.files),
        })
        .collect();
    out.push_str(lang.pick("patterns: ", "minták: "));
    out.push_str(&items.join(", "));
    out.push('\n');
}

/// Ennyi feloldott rétegközi hívás alatt csak akkor van `flow:` sor, ha legalább `MIN_FLOW_PERCENT`%-uk a várt irányba megy.
const MIN_FLOW_CALLS: usize = 10;
const MIN_FLOW_PERCENT: usize = 80;

fn flow_line(out: &mut String, profile: &ProjectProfile, lang: Lang) {
    // kevés ÉS ellentmondásos rétegközi hívásból nem mondunk irányt (zaj); a kevés, de egyértelmű megmarad
    let Some(flow) = profile
        .flow
        .as_ref()
        .filter(|flow| flow.total >= MIN_FLOW_CALLS || flow.percent() >= MIN_FLOW_PERCENT)
    else {
        return;
    };
    let chain = flow
        .chain
        .iter()
        .map(|group| {
            group
                .iter()
                .map(|layer| layer.label(&profile.mixed))
                .collect::<Vec<_>>()
                .join(" → ")
        })
        .collect::<Vec<_>>()
        .join("; ");
    let (percent, total) = (flow.percent(), flow.total);
    let _ = match lang {
        Lang::En => writeln!(
            out,
            "flow: {chain}   (checked: {percent}% of {total} cross-layer calls go this way)"
        ),
        Lang::Hu => writeln!(
            out,
            "irány: {chain}   (ellenőrizve: a {total} rétegek közti hívás {percent}%-a erre megy)"
        ),
    };
}

fn details(out: &mut String, profile: &ProjectProfile, lang: Lang) {
    if !profile.frameworks.is_empty() {
        let list = profile.frameworks.join(", ");
        let _ = writeln!(out, "{}{list}", lang.pick("frameworks: ", "frameworkök: "));
    }
    let with_role: usize = profile.layers.iter().map(|l| l.files).sum();
    let percent = (with_role * 100).checked_div(profile.files).unwrap_or(0);
    let files = profile.files;
    let _ = match lang {
        Lang::En => writeln!(
            out,
            "files: {files} code files, {with_role} with a role ({percent}%)"
        ),
        Lang::Hu => writeln!(
            out,
            "fájlok: {files} kódfájl, ebből {with_role} szereppel ({percent}%)"
        ),
    };
    if !profile.graph_checked {
        out.push_str(lang.pick(
            "(names only: the graph was not available for the inheritance / call-direction check)\n",
            "(csak nevek: a gráf nem volt elérhető az öröklés / hívásirány ellenőrzéséhez)\n",
        ));
    }
    if !profile.layers.is_empty() {
        out.push_str(lang.pick("layers (files):\n", "rétegek (fájlok):\n"));
    }
    for layer in &profile.layers {
        let [high, medium, low] = layer.by_confidence;
        let (h, m, l) = (
            super::role::Confidence::High.label(lang),
            super::role::Confidence::Medium.label(lang),
            super::role::Confidence::Low.label(lang),
        );
        let mut line = format!(
            "  {}  {}  ({h} {high}, {m} {medium}, {l} {low}",
            layer.label,
            layer.files
        );
        if let Some(label) = layer.confirm_label {
            let _ = match lang {
                Lang::En => write!(line, "; confirmed {}: {label}", layer.confirmed),
                Lang::Hu => write!(line, "; megerősítve {}: {label}", layer.confirmed),
            };
        }
        line.push(')');
        out.push_str(&line);
        out.push('\n');
        for (folder, n) in &layer.folders {
            let _ = writeln!(out, "    {folder}  {n}");
        }
    }
    if !profile.patterns.is_empty() {
        out.push_str(lang.pick("patterns:\n", "minták:\n"));
        for pattern in &profile.patterns {
            let _ = writeln!(
                out,
                "  {}  {}  {}",
                pattern.name,
                pattern.files,
                pattern.examples.join(", ")
            );
        }
    }
    flow_line(out, profile, lang);
    if let Some(flow) = &profile.flow {
        for ((from, into), n) in &flow.pairs {
            let mark = if from.role.rank() <= into.role.rank() {
                ""
            } else {
                "  ↑"
            };
            let (from, into) = (from.label(&profile.mixed), into.label(&profile.mixed));
            let _ = writeln!(out, "  {from} → {into}  {n}{mark}");
        }
    }
    if !profile.unknown.is_empty() {
        out.push_str(lang.pick(
            "unknown (no clear role from the names):\n",
            "ismeretlen (a nevekből nincs egyértelmű szerep):\n",
        ));
        for (folder, n) in &profile.unknown {
            let _ = writeln!(out, "  {folder}  {n}");
        }
    }
    skipped_lines(out, profile, lang);
    if let Some(source) = &profile.overrides {
        let _ = writeln!(out, ".callgraph-layers: {}", source.display());
    }
    for warning in &profile.warnings {
        let _ = match lang {
            Lang::En => writeln!(out, "  ⚠ not understood: {warning}"),
            Lang::Hu => writeln!(out, "  ⚠ nem értelmezhető: {warning}"),
        };
    }
}
