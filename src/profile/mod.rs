//! Projekt-profil: az architektúra, a rétegek és a tervezési minták felismerése a mappa- és fájlnevekből (A, B jel),
//! megerősítve a már kész gráfból (C jel: öröklés / interfész, rétegek közti hívásirány). Minden fájl kap egy
//! szerepet bizonyossággal (magas / közepes / alacsony), vagy ismeretlen marad — nem találgat.
//!
//! - `rules.rs`, `role.rs`, `confirm.rs`, `architecture.rs`: a szabályok adat-táblái (bővíthetők, nyelvenként);
//! - `classify.rs`: fájl → szerep, csak az útból (a lekérdezéskor is olcsó; a `.callgraph-layers` felülírhatja);
//! - `project.rs`, `flow.rs`: a projekt összesítése; `build.rs`: bejárás + gráf; `format.rs`: a szöveges kimenet;
//! - `scope.rs`, `view.rs`: a lekérdezések réteg-kapcsolói (`--layer`, `--layers`, `--by-layer`).
//!
//! A meglévő kimenetek alapból nem változnak: minden réteg-viselkedés kapcsolóra vagy külön parancsra (`profile`,
//! MCP: `project_profile`) jön. A fájl rétege az útjából számolódik, ezért a tárolt gráf formátuma nem változik.

mod architecture;
mod build;
mod classify;
mod confirm;
mod ecosystem;
mod flow;
mod format;
mod layers_file;
mod names;
mod project;
mod role;
mod rules;
mod scope;
mod view;

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::i18n::Lang;

pub(crate) use build::{quick, with_graph};
pub(crate) use format::{Tip, render};
pub(crate) use scope::LayerScope;
pub(crate) use view::{layered_summary, tag_file_lines};

/// A tömör, csak a nevekből számolt profil (az MCP-kapcsolódáskor az útmutató végére): gyors, és soha nem bukik
/// el — ha a gyökér nem projekt-mappa (nincs manifest / `.git`), vagy nincs kódfájl, `None`.
pub(crate) fn startup_summary(root: &Path, lang: Lang) -> Option<String> {
    const MARKERS: &[&str] = &[
        "composer.json",
        "package.json",
        "Cargo.toml",
        "Package.swift",
        ".git",
        ".callgraph",
    ];
    if !MARKERS.iter().any(|marker| root.join(marker).exists()) {
        return None;
    }
    let root = root.to_path_buf();
    let profile = std::panic::catch_unwind(move || quick(&root)).ok()?;
    if profile.files == 0 {
        return None;
    }
    Some(render(&profile, lang, false, Tip::Hide))
}
