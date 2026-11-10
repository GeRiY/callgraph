//! A beküldött JSON: a meglévő napi összesítők (ugyanazok a sávos számok, mint a `stats`-ban — út, név, szöveg,
//! azonosító nincs bennük), csak LEZÁRT napok (a mai nem), legfeljebb 7 nap visszamenőleg, és mellé a verzió, az
//! os/arch, napónként egy véletlen kötegazonosító (a szerveroldali duplikátum-szűréshez; nem tároljuk) és a séma
//! verziója.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::stats::day::Day;
use crate::stats::event::CommandKind;
use crate::stats::store::{Counters, StatsStore};

/// A séma verziója.
pub(crate) const SCHEMA: u32 = 1;
/// Ennyi lezárt napot küld legfeljebb visszamenőleg.
pub(crate) const MAX_DAYS: u32 = 7;

/// A beküldés törzse.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Payload {
    pub schema: u32,
    pub version: String,
    pub platform: String,
    pub days: Vec<DayEntry>,
}

/// Egy lezárt nap.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DayEntry {
    pub day: String,
    /// Véletlen kötegazonosító (UUID v4 alakú), naponként új.
    pub batch: String,
    pub commands: BTreeMap<CommandKind, Counters>,
}

/// A beküldésre váró lezárt napok (régebbi elöl): a `today` előtti legfeljebb 7 nap, az utolsó beküldött nap után,
/// amelyekre van adat.
pub(crate) fn pending_days(store: &StatsStore, last_sent: Option<&str>, today: Day) -> Vec<Day> {
    let last = last_sent.and_then(Day::parse);
    (1..=MAX_DAYS)
        .rev()
        .map(|n| today.minus(n))
        .filter(|day| last.is_none_or(|last| *day > last))
        .filter(|day| !store.day_counters(*day).is_empty())
        .collect()
}

/// A törzs a megadott napokra.
pub(crate) fn build(store: &StatsStore, days: &[Day]) -> Payload {
    Payload {
        schema: SCHEMA,
        version: crate::update::build_version().to_owned(),
        platform: crate::update::Platform::current().dir(),
        days: days
            .iter()
            .map(|day| DayEntry {
                day: day.to_string(),
                batch: batch_id(),
                commands: store.day_counters(*day),
            })
            .collect(),
    }
}

/// Véletlen UUID v4 alakú azonosító (a szabványos könyvtár véletlen hash-kulcsaiból).
pub(crate) fn batch_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut words = [0u64; 2];
    for (at, word) in words.iter_mut().enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_usize(at);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        *word = h.finish();
    }
    let hi = (words[0] & 0xffff_ffff_ffff_0fff) | 0x0000_0000_0000_4000;
    let lo = (words[1] & 0x3fff_ffff_ffff_ffff) | 0x8000_0000_0000_0000;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        hi >> 32,
        (hi >> 16) & 0xffff,
        hi & 0xffff,
        lo >> 48,
        lo & 0xffff_ffff_ffff
    )
}
