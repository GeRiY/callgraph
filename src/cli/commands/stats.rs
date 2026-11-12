//! `callgraph stats [--days N]`: a helyi használati statisztika (`crate::stats`) az elmúlt N napról — hívások
//! kliensenként és parancsonként, hibák, idő- és kimenet-méret sávok, a `locate`-találati arány. `--events`: a helyi
//! eseménynapló (hívásonként egy sor, legfeljebb 14 nap) és a leggyakoribb parancs-átmenetek. Csak olvas; a saját
//! futását nem méri. Hálózat nincs (a beküldés opt-in: `telemetry`).

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::SkillHome;
use crate::stats::{Day, StatsEnv, StatsStore, render, render_events};

/// A statisztika-parancs; az adatmappa a `SkillHome`-ból (vagy a `CALLGRAPH_STATS_DIR`-ből).
pub struct StatsCommand {
    days: u32,
    events: bool,
    home: SkillHome,
}

impl StatsCommand {
    pub fn new(days: u32, home: SkillHome) -> Self {
        Self {
            days,
            events: false,
            home,
        }
    }

    /// `--events`: az eseménynapló a riport helyett.
    pub fn with_events(mut self, events: bool) -> Self {
        self.events = events;
        self
    }
}

impl Command for StatsCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let env = StatsEnv::from_process();
        let store = StatsStore::new(env.data_dir(&self.home));
        let today = Day::today();
        if self.events {
            out.write_all(render_events(&store, today, self.days).as_bytes())?;
            return Ok(0);
        }
        let totals = store.totals(today.minus(self.days.saturating_sub(1)), today);
        let mut text = render(&totals, today, self.days, store.dir(), env.enabled());
        // a „most nem” utáni egyetlen emlékeztető a beküldésről (tipp a paranccsal, nem kérdés)
        let telemetry = crate::stats::telemetry::Telemetry::from_env(&self.home);
        if let Some(hint) = telemetry.stats_hint(crate::stats::telemetry::now_secs()) {
            text.push_str(&hint);
        }
        out.write_all(text.as_bytes())?;
        Ok(0)
    }
}
