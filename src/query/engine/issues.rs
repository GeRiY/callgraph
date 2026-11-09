//! A lekérdezés során beolvasott, szintaktikai hibás fájlok gyűjtése a kimenet záró `⚠` sorához: a `--grep` a
//! szűrés után csak a találatokat adja vissza, a hibás (esetleg ezért találat nélküli) fájlokat viszont itt is
//! számon tartjuk, hogy a „nincs hívó” ne tűnjön biztosnak.

use std::path::Path;
use std::sync::PoisonError;

use super::QueryEngine;
use crate::model::FileGraph;

impl QueryEngine {
    /// Az eddigi lekérdezésekben beolvasott, szintaktikai hibás fájlok (gyökérhez relatív út, az első hiba sora), a
    /// beolvasás sorrendjében; a záró sorhoz: `crate::query::format::parse_trailer`.
    pub fn parse_issues(&self) -> Vec<(String, u32)> {
        self.parse_issues
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Egy beolvasott fájl feljegyzése, ha szintaktikai hibás (egy fájl egyszer).
    pub(crate) fn note_parse_issue(&self, path: &Path, graph: &FileGraph) {
        let Some(issue) = crate::query::format::parse_issue(&self.root, path, graph) else {
            return;
        };
        let mut issues = self
            .parse_issues
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !issues.iter().any(|(seen, _)| *seen == issue.0) {
            issues.push(issue);
        }
    }
}
