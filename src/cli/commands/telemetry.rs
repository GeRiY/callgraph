//! `callgraph telemetry on|off|status|preview` — a helyi statisztika opt-in beküldése (`stats::telemetry`). Az `on`
//! csak interaktív terminálban, y/N megerősítéssel működik (agent nem kapcsolhatja be). Belső alparancsok: `send`
//! (a leválasztott háttér-beküldés) és `ask` (a telepítés / frissítés végi kérdés, csak terminálban).

use std::io::Write;

use crate::cli::{Command, TelemetryAction};
use crate::error::{CallgraphError, Result};
use crate::skill::SkillHome;
use crate::stats::telemetry::{
    Choice, HttpSender, SendOutcome, Telemetry, Tty, decided_line, now_secs,
};

/// A `telemetry` parancs.
pub struct TelemetryCommand {
    action: TelemetryAction,
    home: SkillHome,
}

impl TelemetryCommand {
    pub fn new(action: TelemetryAction, home: SkillHome) -> Self {
        Self { action, home }
    }
}

impl Command for TelemetryCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let telemetry = Telemetry::from_env(&self.home);
        let now = now_secs();
        match self.action {
            TelemetryAction::Status => write!(out, "{}", telemetry.status(now))?,
            TelemetryAction::Preview => writeln!(out, "{}", telemetry.preview(now))?,
            TelemetryAction::Off => {
                telemetry.decide(Choice::Off, now)?;
                writeln!(out, "{}", decided_line(Choice::Off))?;
            }
            TelemetryAction::On => {
                let stdin = std::io::stdin();
                let on = telemetry
                    .confirm_on(Tty::from_process(), &mut stdin.lock(), err, now)
                    .map_err(CallgraphError::Usage)?;
                let line = if on {
                    decided_line(Choice::Yes)
                } else {
                    crate::t!("Nothing changed.", "Nem változott semmi.")
                };
                writeln!(out, "{line}")?;
            }
            // a leválasztott háttérfolyamatban fut (null stdout/stderr); kézzel futtatva látszik az eredmény
            TelemetryAction::Send => match telemetry.send_now(&HttpSender, now) {
                SendOutcome::Sent(days) => writeln!(out, "telemetry: sent {days} day(s)")?,
                SendOutcome::Failed(reason) => writeln!(err, "telemetry: {reason}")?,
                SendOutcome::Skipped => {}
            },
            TelemetryAction::Ask => {
                let stdin = std::io::stdin();
                telemetry.ask(Tty::from_process(), &mut stdin.lock(), err, now);
            }
        }
        Ok(0)
    }
}
