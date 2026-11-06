//! Mikor mérünk, és hová írunk. Ugyanaz a szabály, mint a frissítés-ellenőrzésnél (`update::check::CheckEnv`):
//! csak skill-telepítésből futva, `CI` nélkül; `CALLGRAPH_STATS=0|off|false|no` mindig kikapcsol. A
//! `CALLGRAPH_STATS_DIR` kifejezett bekapcsolás egy megadott mappába (a tesztek ezzel mérnek egy ideiglenes
//! mappába) — ilyenkor a `CI` és a telepítés-vizsgálat nem számít, a `CALLGRAPH_STATS=0` igen.

use std::path::PathBuf;

use crate::skill::SkillHome;

/// A kikapcsoló változó.
pub(crate) const SETTING_ENV: &str = "CALLGRAPH_STATS";
/// A kifejezett adatmappa (bekapcsol).
pub(crate) const DIR_ENV: &str = "CALLGRAPH_STATS_DIR";

/// A mérést befolyásoló környezet.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StatsEnv {
    /// A `CALLGRAPH_STATS` értéke.
    pub(crate) setting: Option<String>,
    /// A `CALLGRAPH_STATS_DIR` (nem üres) értéke.
    pub(crate) dir: Option<PathBuf>,
    /// Be van-e állítva a `CI`.
    pub(crate) ci: bool,
    /// Skill-telepítésből fut-e a bináris.
    pub(crate) in_skill: bool,
}

impl StatsEnv {
    /// A folyamat környezetéből; a skill-telepítést csak akkor vizsgálja, ha a döntés múlik rajta.
    pub(crate) fn from_process() -> Self {
        let mut env = Self {
            setting: std::env::var(SETTING_ENV).ok(),
            dir: std::env::var_os(DIR_ENV)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            ci: std::env::var_os("CI").is_some_and(|v| !v.is_empty()),
            in_skill: false,
        };
        if !env.opted_out() && env.dir.is_none() && !env.ci {
            env.in_skill = crate::update::InstallTarget::current().is_some();
        }
        env
    }

    /// Kifejezetten kikapcsolta-e a felhasználó.
    pub(crate) fn opted_out(&self) -> bool {
        self.setting.as_deref().is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "off" | "false" | "no"
            )
        })
    }

    /// Mérünk-e.
    pub(crate) fn enabled(&self) -> bool {
        !self.opted_out() && (self.dir.is_some() || (!self.ci && self.in_skill))
    }

    /// Az adatmappa: a `CALLGRAPH_STATS_DIR`, különben `<skill>/stats`.
    pub(crate) fn data_dir(&self, home: &SkillHome) -> PathBuf {
        self.dir.clone().unwrap_or_else(|| home.stats_dir())
    }
}
