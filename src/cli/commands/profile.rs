//! `callgraph profile [--project <alias|mappa>] [--full]`: a projekt-profil — architektúra, rétegek mappánként,
//! tervezési minták, ismeretlen mappák és a rétegek közti hívásirány (`crate::profile`). Tömör (~15 sor)
//! alapból; `--full`: mappánként, bizonyosság szerint, a hívásirány párjaival.

use std::io::Write;
use std::path::Path;

use crate::cli::Command;
use crate::error::Result;
use crate::profile::{Tip, render, with_graph};
use crate::skill::SkillHome;

/// A profil parancsa; a `--project` aliast a `SkillHome` registryje oldja fel (különben mappa-út, vagy a
/// munkakönyvtár).
pub struct ProfileCommand {
    project: Option<String>,
    full: bool,
    home: SkillHome,
}

impl ProfileCommand {
    pub fn new(project: Option<String>, full: bool, home: SkillHome) -> Self {
        Self {
            project,
            full,
            home,
        }
    }
}

impl Command for ProfileCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let profile = with_graph(&root);
        let lang = crate::i18n::lang();
        write!(out, "{}", render(&profile, lang, self.full, Tip::Show))?;
        // a gráf-alapú megerősítés hibás fájlnál hiányos: a záró `⚠` sor (hiba nélkül üres)
        let issues = parse_issues(&root);
        write!(out, "{}", crate::query::format::parse_trailer(&issues, lang))?;
        Ok(0)
    }
}

/// A projekt-gráfban (a `with_graph` frissítette) a szintaktikai hibás fájlok: gyökérhez relatív út, az első hiba
/// sora, út szerint rendezve; gráf nélkül üres.
fn parse_issues(root: &Path) -> Vec<(String, u32)> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let Some(shared) = crate::graph::shared(&root) else {
        return Vec::new();
    };
    let graph = crate::graph::lock(&shared);
    graph
        .tracked_under(&root)
        .iter()
        .filter_map(|key| {
            let stored = graph.slim_graph(key)?;
            crate::query::format::parse_issue(&root, Path::new(key), stored)
        })
        .collect()
}
