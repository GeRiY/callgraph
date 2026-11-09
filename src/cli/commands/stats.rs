//! `callgraph stats [--days N]`: a helyi használati statisztika (`crate::stats`) az elmúlt N napról — hívások
//! kliensenként és parancsonként, hibák, idő- és kimenet-méret sávok, a `locate`-találati arány. Csak olvas; a
//! saját futását nem méri. Hálózat nincs.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::SkillHome;
use crate::stats::{Day, StatsEnv, StatsStore, render};

/// A statisztika-parancs; az adatmappa a `SkillHome`-ból (vagy a `CALLGRAPH_STATS_DIR`-ből).
pub struct StatsCommand {
    days: u32,
    home: SkillHome,
}

impl StatsCommand {
    pub fn new(days: u32, home: SkillHome) -> Self {
        Self { days, home }
    }
}

impl Command for StatsCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let env = StatsEnv::from_process();
        let store = StatsStore::new(env.data_dir(&self.home));
        let today = Day::today();
        let totals = store.totals(today.minus(self.days.saturating_sub(1)), today);
        let text = render(&totals, today, self.days, store.dir(), env.enabled());
        out.write_all(text.as_bytes())?;
        Ok(0)
    }
}
