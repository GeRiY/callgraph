//! Helyi, passzív használati mérés és a `callgraph stats` riport (a telemetria-terv 1. fázisa: hálózat NINCS,
//! semmi nem hagyja el a gépet).
//!
//! Minden CLI-parancs (`main.rs`: `CliProbe`) és minden MCP-eszköz-hívás (`mcp::server`: `McpStats`) után egy
//! napi összesítőbe kerül: a parancs neve a rögzített listából, a kliens (cli / mcp / hook), siker vagy a hiba
//! kategóriája, a futási idő és a kimenet-méret sávja, levágta-e a kimeneti korlát, és MCP-ben a `locate`
//! rangsor-találata. Út, fájl-, szimbólum- és projektnév, keresési kifejezés, hibaüzenet és ezek hash-e SOHA:
//! a típusok (`event.rs`) csak zárt felsorolásokat és sávokat hordoznak.
//!
//! Tárolás: `<skill>/stats/YYYY-MM.json` (`store.rs`), ~90 napig. Kikapcsolva: `CALLGRAPH_STATS=0|off|false`,
//! `CI`, és skill-telepítésen kívül (`policy.rs`); a `CALLGRAPH_STATS_DIR` kifejezetten bekapcsol egy mappába.

mod buckets;
mod cli_probe;
mod day;
mod event;
mod locate_hits;
mod mcp_probe;
mod policy;
mod report;
mod store;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tracking_tests;

pub(crate) use cli_probe::CLIENT_ENV;
pub use cli_probe::{CliProbe, Meter};
pub(crate) use day::Day;
pub(crate) use event::CommandKind;
pub(crate) use mcp_probe::{McpStats, ToolCall};
pub(crate) use policy::StatsEnv;
pub(crate) use report::render;
pub(crate) use store::StatsStore;

use buckets::RankBucket;
use event::Event;

use crate::skill::SkillHome;

/// A `callgraph stats --days` alapértéke és felső határa.
pub(crate) const DEFAULT_DAYS: u32 = 7;
pub(crate) const MAX_DAYS: u32 = store::RETENTION_DAYS;

/// A rögzítő: kikapcsolt környezetben semmit nem ír (egy fájlt sem nyit).
#[derive(Debug, Clone)]
pub(crate) struct Recorder {
    store: Option<StatsStore>,
}

impl Recorder {
    /// A folyamat környezetéből (`StatsEnv`).
    pub(crate) fn from_env(home: &SkillHome) -> Self {
        Self::new(&StatsEnv::from_process(), home)
    }

    pub(crate) fn new(env: &StatsEnv, home: &SkillHome) -> Self {
        Self {
            store: env.enabled().then(|| StatsStore::new(env.data_dir(home))),
        }
    }

    /// Egy megadott mappába ír (tesztek).
    #[cfg(test)]
    pub(crate) fn at(dir: std::path::PathBuf) -> Self {
        Self {
            store: Some(StatsStore::new(dir)),
        }
    }

    /// Nem ír semmit.
    pub(crate) fn disabled() -> Self {
        Self { store: None }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.store.is_some()
    }

    /// Egy hívás rögzítése a mai (UTC) napra; minden hibát elnyel.
    pub(crate) fn record(&self, event: &Event) {
        if let Some(store) = &self.store {
            store.record(Day::today(), event);
        }
    }

    /// Egy `locate`-rangsor-találat rögzítése a mai napra.
    pub(crate) fn record_hit(&self, hit: RankBucket) {
        if let Some(store) = &self.store {
            store.record_hit(Day::today(), hit);
        }
    }
}
