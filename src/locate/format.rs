//! A locate szöveges kimenete: fejléc (`locate: N terms · M matches in K files · start here ↓`), a rangsor első
//! fájljai (alapból 5, `--all`-lal mind) fájlonként legfeljebb 3 indoklással, végül a találat nélküli és a túl
//! általános kifejezések, és hogy van-e még fájl.

use std::collections::HashMap;

use super::rank::{FileMatch, stars};
use super::reasons::{describe, group, pick};
use super::terms::{Term, TermKind};

/// Alapból ennyi fájl jelenik meg.
pub(crate) const DEFAULT_FILES: usize = 5;
/// Fájlonként legfeljebb ennyi indoklás-sor.
const REASONS_PER_FILE: usize = 3;
/// Ennyi fájlnál több találat: a kifejezés túl általános.
pub(crate) const TOO_GENERAL: usize = 30;

/// A kiírandó eredmény: a kifejezések és a rangsorolt fájlok.
pub(crate) struct Report<'a> {
    pub terms: &'a [Term],
    pub ranked: &'a [FileMatch],
    pub all: bool,
    /// Fájlonként a (nem árva) jegyzetek száma; a fájl-sor végén `  📝 N`.
    pub notes: &'a HashMap<String, usize>,
}

impl Report<'_> {
    /// A teljes szöveg (az utak úgy, ahogy a `FileMatch`-ben állnak).
    pub(crate) fn render(&self) -> String {
        let mut out = String::new();
        let n = self.terms.len();
        let k = self.ranked.len();
        if k == 0 {
            out.push_str(&crate::tf!(
                "locate: {n} terms · no match in the project map\n",
                "locate: {n} kifejezés · nincs találat a projekt-térképen\n"
            ));
        } else {
            let m: usize = self.ranked.iter().map(|file| file.hits.len()).sum();
            out.push_str(&crate::tf!(
                "locate: {n} terms · {m} matches in {k} files · start here ↓\n",
                "locate: {n} kifejezés · {m} találat {k} fájlban · innen indulj ↓\n"
            ));
        }
        let shown = if self.all { k } else { k.min(DEFAULT_FILES) };
        let top = self.ranked.first().map_or(0, |file| file.score);
        for (rank, file) in self.ranked.iter().take(shown).enumerate() {
            let notes = self.notes.get(&file.path).copied().unwrap_or(0);
            out.push_str(&format!(
                "{}. {}  {}{}\n",
                rank + 1,
                file.path,
                stars(file.score, top),
                crate::notes::mark(notes)
            ));
            for reason in pick(group(&file.hits), REASONS_PER_FILE) {
                out.push_str(&format!("   {}\n", describe(&reason, self.terms)));
            }
        }
        self.footer(&mut out, shown);
        out
    }

    /// A záró sorok: találat nélküli és túl általános kifejezések, további fájlok.
    fn footer(&self, out: &mut String, shown: usize) {
        let mut files_per_term = vec![0usize; self.terms.len()];
        for file in self.ranked {
            for term in file.terms() {
                if let Some(count) = files_per_term.get_mut(term) {
                    *count += 1;
                }
            }
        }
        let names = |keep: &dyn Fn(usize) -> bool| -> Vec<String> {
            self.terms
                .iter()
                .enumerate()
                .filter(|(i, _)| keep(files_per_term[*i]))
                .map(|(_, term)| term.raw.clone())
                .collect()
        };
        let missing = names(&|count| count == 0);
        if !missing.is_empty() {
            let list = missing.join(", ");
            out.push_str(&crate::tf!("no match: {list}\n", "nincs találat: {list}\n"));
        }
        // a találat nélküli útvonalnévnél: mit kerestünk
        for (i, term) in self.terms.iter().enumerate() {
            let TermKind::Route(name) = &term.kind else {
                continue;
            };
            if files_per_term[i] == 0 {
                out.push_str(&crate::tf!(
                    "  {}: searched the definition ->name('{name}') in routes/*.php and the string '{name}' in PHP / JS / TS / Vue (route(), to_route(), redirect()->route() arguments)\n",
                    "  {}: keresve a ->name('{name}') definíció a routes/*.php-ban és a '{name}' string a PHP / JS / TS / Vue fájlokban (route(), to_route(), redirect()->route() argumentumai)\n",
                    term.raw
                ));
            }
        }
        let general = names(&|count| count > TOO_GENERAL);
        if !general.is_empty() {
            let list = general.join(", ");
            out.push_str(&crate::tf!(
                "too general (matched in more than {TOO_GENERAL} files): {list} — add a more specific term\n",
                "túl általános (több mint {TOO_GENERAL} fájlban): {list} — adj meg pontosabbat\n"
            ));
        }
        let total = self.ranked.len();
        if shown < total {
            out.push_str(&crate::tf!(
                "more: --all ({total} files)\n",
                "több: --all ({total} fájl)\n"
            ));
        }
    }
}
