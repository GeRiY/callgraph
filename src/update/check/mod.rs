//! Automatikus frissítés-ellenőrzés a `wrap` (a `callgraph.sh`) futásai után, lassítás nélkül.
//!
//! A forró úton csak egy kis állapotfájl olvasása / írása történik (`<skill>/update-check.json`), és naponta
//! legfeljebb egyszer egy várakozás nélküli folyamat-indítás: a gyerek (`self-update --check --background`)
//! végzi a hálózati lekérdezést, és beírja a `latest` mezőt. Ha az állapot szerint újabb verzió van, a sikeres
//! kimenet végére egy egysoros értesítés kerül (verziónként legfeljebb 24 óránként).
//! Kikapcsolás: `CALLGRAPH_UPDATE_CHECK=0|off|false`; automatikusan ki van kapcsolva `CI` mellett és skill-telepítésen kívül.

mod policy;
mod spawn;
mod state;

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
pub(crate) use policy::INTERVAL_SECS;
pub(crate) use policy::{CheckEnv, Decision, decide, decide_auto};
pub(crate) use spawn::{
    BACKGROUND_ARGS, DetachedProcess, Spawner, spawn_detached, spawn_detached_as,
};
pub(crate) use state::{CheckState, STATE_FILE};

use super::{InstallTarget, Version};
use crate::skill::SkillHome;

/// A jelenlegi unix-idő másodpercben.
pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl CheckEnv {
    /// A folyamat környezetéből; a skill-telepítést csak akkor vizsgálja, ha másképp nem lenne kikapcsolva.
    pub(crate) fn from_process() -> Self {
        let mut env = Self {
            setting: std::env::var("CALLGRAPH_UPDATE_CHECK").ok(),
            ci: std::env::var_os("CI").is_some_and(|v| !v.is_empty()),
            in_skill: true,
        };
        if env.enabled() {
            env.in_skill = InstallTarget::current().is_some();
        }
        env
    }
}

/// Az értesítő sor (egy sor, a tokenek miatt): `ℹ callgraph 0.2.0 is available (installed: 0.1.0) — update: …`.
pub(crate) fn notice_line(latest: &Version, current: &Version, update_cmd: &str) -> String {
    crate::tf!(
        "ℹ callgraph {latest} is available (installed: {current}) — update: {update_cmd} update",
        "ℹ callgraph {latest} elérhető (telepítve: {current}) — frissítés: {update_cmd} update"
    )
}

/// A `wrap` utáni ellenőrzés a függőségeivel (DI): állapotfájl, környezet, indító, saját verzió, frissítő parancs.
pub(crate) struct UpdateCheck {
    file: PathBuf,
    env: CheckEnv,
    spawner: Box<dyn Spawner>,
    current: Version,
    update_cmd: String,
}

impl UpdateCheck {
    /// A valódi környezet: a skill-mappa állapotfájlja, a folyamat környezete, leválasztott gyerekfolyamat.
    pub(crate) fn from_env(home: &SkillHome) -> Self {
        Self::new(home, CheckEnv::from_process(), Box::new(DetachedProcess))
    }

    pub(crate) fn new(home: &SkillHome, env: CheckEnv, spawner: Box<dyn Spawner>) -> Self {
        Self {
            file: home.update_check_file(),
            env,
            spawner,
            current: Version::current(),
            update_cmd: home.script("callgraph.sh"),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_current(mut self, current: Version) -> Self {
        self.current = current;
        self
    }

    /// A parancs utáni lépés: állapot mentése, szükség esetén háttér-ellenőrzés, értesítés a kimenet végére.
    /// Semmilyen hibája nem érinti a parancs eredményét.
    pub(crate) fn after_command(&self, now: u64, succeeded: bool, out: &mut dyn Write) {
        if !self.env.enabled() {
            return;
        }
        let state = CheckState::load(&self.file);
        let Decision {
            state: next,
            spawn,
            notice,
        } = decide(&state, now, &self.current, succeeded);
        // mentés nélkül (pl. írásvédett mappa) nem indítunk: különben minden futás indítana egyet
        let saved = next == state || next.save(&self.file).is_ok();
        if spawn && saved {
            self.spawner.spawn();
        }
        if let Some(latest) = notice {
            let _ = writeln!(
                out,
                "{}",
                notice_line(&latest, &self.current, &self.update_cmd)
            );
        }
    }
}

/// A háttér-gyerek eredményének rögzítése: `latest` (vagy `None`, ha nincs kiadás) és `checked_at`;
/// a hívó hibánál (`Err`) csak az időbélyeget frissíti. Minden hibát elnyel.
pub(crate) fn record_latest(home: &SkillHome, now: u64, latest: Result<Option<Version>, ()>) {
    let file = home.update_check_file();
    let mut state = CheckState::load(&file);
    state.checked_at = now;
    if let Ok(latest) = latest {
        state.latest = latest.map(|v| v.to_string());
    }
    let _ = state.save(&file);
}

#[cfg(test)]
mod tests;
