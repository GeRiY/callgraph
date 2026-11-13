//! Helyi, passzív használati mérés és a `callgraph stats` riport. Hálózat csak az opt-in beküldésnél van
//! (`telemetry`: alapból KI, csak a felhasználó kapcsolhatja be).
//!
//! Minden CLI-parancs (`main.rs`: `CliProbe`) és minden MCP-eszköz-hívás (`mcp::server`: `McpStats`) után egy
//! napi összesítőbe kerül: a parancs neve a rögzített listából, a kliens (cli / mcp / hook), siker vagy a hiba
//! kategóriája, a futási idő és a kimenet-méret sávja, levágta-e a kimeneti korlát, és MCP-ben a `locate`
//! rangsor-találata. Út, fájl-, szimbólum- és projektnév, keresési kifejezés, hibaüzenet és ezek hash-e SOHA:
//! a típusok (`event.rs`) csak zárt felsorolásokat és sávokat hordoznak.
//!
//! Tárolás: `<skill>/stats/YYYY-MM.json` (`store.rs`), ~90 napig. Kikapcsolva: `CALLGRAPH_STATS=0|off|false`,
//! `CI`, és skill-telepítésen kívül (`policy.rs`); a `CALLGRAPH_STATS_DIR` kifejezetten bekapcsol egy mappába.
//!
//! A napi összesítőben naponként a parancs-sorrend is ott van (`edges`: melyik parancs után melyik jött, a cél
//! viszonya — ugyanaz a fájl / mappa / projekt — és a szünet sávja), a lezárt munkamenetek sávjai (`sessions`), és
//! parancsonként a célfájl nyelve (rögzített lista) és méret-sávja — ezek is csak nevek és sávok.
//!
//! CSAK helyben, soha nem megy el: az eseménynapló (`events/YYYY-MM-DD.jsonl`, 14 nap, `events.rs`: hívásonként
//! egy sor, nevek nélkül) és a munkamenet-állapot (`session.json`, `session.rs`). Utakból származó adat egyedül
//! az utóbbiban van: kulcsolt hash (HMAC-SHA256 egy telepítésenkénti véletlen kulccsal, ami szintén nem hagyja el a
//! gépet), kizárólag a `rel` / `rep` jelzők kiszámításához; ugyanígy az utolsó hívás kimenetének elemei (fájl-utak,
//! azonosítók) rövidített kulcsolt hash-ként, kizárólag a `use_file` / `use_sym` sávokhoz (`reuse.rs`: a következő
//! hívás célja benne volt-e az előző kimenetben). A napi összesítőbe ebből csak a sáv kerül (`uses`).

mod buckets;
mod cli_probe;
mod day;
mod event;
mod events;
mod hmac;
mod locate_hits;
mod mcp_probe;
mod policy;
mod report;
mod reuse;
mod session;
mod store;
mod target;
pub(crate) mod telemetry;

#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod reuse_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tracking_tests;

pub(crate) use cli_probe::{CLIENT_ENV, note_failure};
pub use cli_probe::{CliProbe, Meter};
pub(crate) use day::Day;
pub(crate) use event::{CommandKind, truncated};
pub(crate) use mcp_probe::{McpStats, ToolCall};
pub(crate) use policy::StatsEnv;
pub(crate) use report::{render, render_events};
pub(crate) use reuse::single_identifier;
pub(crate) use store::StatsStore;

use buckets::RankBucket;
use target::Call;

use crate::skill::SkillHome;

/// Az egyszeri tájékoztatás (stderr, csak CLI-n; MCP-n soha: az az agent csatornája).
pub(crate) fn notice_text() -> String {
    crate::t!(
        "ℹ callgraph keeps local usage counts on this machine only (nothing is sent) — see: callgraph.sh stats · off: CALLGRAPH_STATS=0",
        "ℹ a callgraph helyi használati számlálókat vezet, csak ezen a gépen (semmi nem megy el) — lásd: callgraph.sh stats · kikapcsolás: CALLGRAPH_STATS=0"
    )
    .to_owned()
}

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

    /// Egy mért hívás rögzítése (napi számlálók, munkamenet, átmenet-él, helyi eseménynapló); minden hibát elnyel.
    pub(crate) fn record_call(&self, call: &Call) {
        if let Some(store) = &self.store {
            store.record_call(call);
        }
    }
    /// Az egyszeri tájékoztatás szövege, ha ezen a gépen most rögzítünk először (a jelzőfájl most jött létre).
    pub(crate) fn first_notice(&self) -> Option<String> {
        let store = self.store.as_ref()?;
        store.claim_notice().then(notice_text)
    }

    /// Egy javaslat-sor / elfogadott javaslat (`next`) rögzítése a mai napra.
    pub(crate) fn record_next(&self, command: CommandKind, used: bool) {
        if let Some(store) = &self.store {
            store.record_next(Day::today(), command, used);
        }
    }

    /// Egy `locate`-rangsor-találat rögzítése a mai napra.
    pub(crate) fn record_hit(&self, hit: RankBucket) {
        if let Some(store) = &self.store {
            store.record_hit(Day::today(), hit);
        }
    }
}
