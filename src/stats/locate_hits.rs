//! A `locate` rangsor-találata egy MCP-munkamenetben: az utolsó `locate` rangsorolt fájl-listája CSAK a memóriában
//! él; ha a következő hívások egyike (`outline` / `method_body` / `find_callers`) a lista egyik fájlját célozza,
//! annak helye lesz a találat (1., 2–5., 6+.). Ha `WINDOW` híváson belül nem nyit meg listán lévő fájlt, vagy új
//! `locate` jön, a találat „nem volt a listán”. Kifelé csak a sáv megy (`RankBucket`), út soha.

use super::buckets::RankBucket;

/// Ennyi további hívás után a nyitott lista „nem volt a listán” eredménnyel zárul.
pub(crate) const WINDOW: usize = 5;

/// A még nyitott lista.
#[derive(Debug)]
struct Pending {
    project: Option<String>,
    files: Vec<String>,
    calls: usize,
}

/// Az utolsó `locate`-lista követése (munkamenetenként egy, memóriában).
#[derive(Debug, Default)]
pub(crate) struct LocateHits {
    pending: Option<Pending>,
}

impl LocateHits {
    /// Új `locate`-eredmény a rangsorolt fájlokkal; az előző, még nyitott lista „nem volt a listán” lesz.
    /// Üres listát nem követ (nincs mit rangsorolni).
    pub(crate) fn listed(
        &mut self,
        project: Option<&str>,
        files: Vec<String>,
    ) -> Option<RankBucket> {
        let previous = self.pending.take().map(|_| RankBucket::NotListed);
        if !files.is_empty() {
            self.pending = Some(Pending {
                project: project.map(str::to_owned),
                files: files.into_iter().map(|f| normalize(&f)).collect(),
                calls: 0,
            });
        }
        previous
    }

    /// Egy későbbi (nem `locate`) hívás; `opened`: a megnyitott fájl, ha a hívás fájlt céloz. A lezárt lista
    /// találata, ha most zárult.
    pub(crate) fn followed(
        &mut self,
        project: Option<&str>,
        opened: Option<&str>,
    ) -> Option<RankBucket> {
        let pending = self.pending.as_mut()?;
        pending.calls += 1;
        let same_project = pending.project.as_deref() == project;
        let rank = opened
            .filter(|_| same_project)
            .map(normalize)
            .and_then(|opened| pending.files.iter().position(|f| same_file(&opened, f)));
        if let Some(index) = rank {
            self.pending = None;
            return Some(RankBucket::from_rank(index + 1));
        }
        if pending.calls >= WINDOW {
            self.pending = None;
            return Some(RankBucket::NotListed);
        }
        None
    }
}

/// A `locate` kimenetének rangsorolt fájljai (`1. src/Order.php  ★★★` sorok), sorrendben.
pub(crate) fn ranked_files(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let (number, rest) = line.split_once(". ")?;
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let path = rest.split_once("  ").map_or(rest, |(path, _)| path).trim();
            (!path.is_empty()).then(|| path.to_owned())
        })
        .collect()
}

/// Egységes alak: `/` elválasztó, `./` előtag és záró `/` nélkül.
fn normalize(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut path = path.as_str();
    while let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    path.trim_end_matches('/').to_owned()
}

/// Ugyanaz a fájl: egyezik, vagy az egyik a másik (gyökér-relatív) útjára végződik (abszolút út a kérésben).
fn same_file(opened: &str, listed: &str) -> bool {
    opened == listed
        || opened.ends_with(&format!("/{listed}"))
        || listed.ends_with(&format!("/{opened}"))
}
