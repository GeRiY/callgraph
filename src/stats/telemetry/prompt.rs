//! A felhasználó kérdezése és a ritka szólások: a `telemetry on` megerősítése (csak terminálban), a telepítés /
//! frissítés végi kérdés, a „most nem” utáni egy emlékeztető és az agentnek szóló tipp.

use std::io::{BufRead, Write};

use super::{Choice, MAX_ASKS, MAX_HINTS, MIN_CALLS, REMIND_AFTER, Telemetry, Tty};
use crate::stats::day::Day;

impl Telemetry {
    /// `telemetry on`: csak interaktív terminálban, y/N megerősítéssel (agent nem kapcsolhatja be); `Ok(true)`, ha
    /// bekapcsolta, `Ok(false)`, ha nemet mondott; `Err`: nem interaktív / itt tiltott (a szöveg a hibaüzenet).
    pub(crate) fn confirm_on(
        &self,
        tty: Tty,
        input: &mut dyn BufRead,
        err: &mut dyn Write,
        now: u64,
    ) -> Result<bool, String> {
        if !tty.interactive() {
            return Err(crate::t!(
                "telemetry on needs an interactive terminal: only the user can turn it on (run `callgraph.sh telemetry on` yourself in a terminal; an agent must not do it).",
                "A telemetry on interaktív terminált kér: csak a felhasználó kapcsolhatja be (futtasd magad terminálban: `callgraph.sh telemetry on`; agent ezt nem teheti meg)."
            )
            .to_owned());
        }
        if !self.env.allowed() {
            return Err(crate::t!(
                "Sending is blocked here (CALLGRAPH_TELEMETRY=0, CALLGRAPH_STATS=0, CI, or not a skill install).",
                "A beküldés itt tiltva (CALLGRAPH_TELEMETRY=0, CALLGRAPH_STATS=0, CI, vagy nem skill-telepítés)."
            )
            .to_owned());
        }
        let _ = write!(
            err,
            "{}",
            crate::t!(
                "Send the daily usage counts (closed days, the same banded numbers as `callgraph.sh stats`; preview: callgraph.sh telemetry preview)? [y/N]: ",
                "Beküldjem a napi használati számokat (lezárt napok, ugyanazok a sávos számok, mint a `callgraph.sh stats`-ban; előnézet: callgraph.sh telemetry preview)? [i/N]: "
            )
        );
        let _ = err.flush();
        let mut answer = String::new();
        let _ = input.read_line(&mut answer);
        if !matches!(
            answer.trim().to_ascii_lowercase().as_str(),
            "y" | "yes" | "i" | "igen"
        ) {
            return Ok(false);
        }
        self.decide(Choice::Yes, now).map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// A telepítés / frissítés végi kérdés, ha még nem döntött és interaktív a futás: Igen / Most nem / Soha.
    /// `true`, ha kérdezett.
    pub(crate) fn ask(
        &self,
        tty: Tty,
        input: &mut dyn BufRead,
        err: &mut dyn Write,
        now: u64,
    ) -> bool {
        let mut consent = self.consent();
        if !self.env.allowed() || !tty.interactive() || consent.choice.is_some() {
            return false;
        }
        let _ = write!(
            err,
            "{}",
            crate::t!(
                "callgraph can send its daily usage counts (the same banded numbers as `callgraph.sh stats`; no paths, names or code) to help improve it. Preview: callgraph.sh telemetry preview\nSend them? [y] yes / [l] not now / [n] never: ",
                "A callgraph beküldheti a napi használati számait (ugyanazok a sávos számok, mint a `callgraph.sh stats`-ban; út, név, kód nélkül), hogy jobb legyen. Előnézet: callgraph.sh telemetry preview\nBeküldhetem? [i] igen / [k] most nem / [n] soha: "
            )
        );
        let _ = err.flush();
        let mut answer = String::new();
        let _ = input.read_line(&mut answer);
        let choice = match answer.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" | "i" | "igen" => Choice::Yes,
            "n" | "never" | "no" | "soha" => Choice::Never,
            _ => Choice::Later,
        };
        consent.decide(choice, now);
        consent.asked_count += 1;
        let _ = self.save(&consent);
        let _ = writeln!(err, "{}", decided_line(choice));
        true
    }

    /// A „most nem” utáni egyetlen emlékeztető a `stats` kimenetében (~14 nap múlva); megjelenéskor számolja.
    pub(crate) fn stats_hint(&self, now: u64) -> Option<String> {
        let mut consent = self.consent();
        let due = self.env.allowed()
            && consent.choice == Some(Choice::Later)
            && consent.asked_count < MAX_ASKS
            && consent
                .decided_at
                .is_some_and(|t| now.saturating_sub(t) >= REMIND_AFTER);
        if !due {
            return None;
        }
        consent.asked_count += 1;
        self.save(&consent).ok()?;
        Some(
            crate::t!(
                "Sharing these counts would help improve callgraph: callgraph.sh telemetry preview (what would be sent) · telemetry on (send) · telemetry off (stop asking)\n",
                "Ha megosztod ezeket a számokat, az segít jobbá tenni a callgraph-ot: callgraph.sh telemetry preview (mi menne el) · telemetry on (beküldés) · telemetry off (ne kérdezzen többet)\n"
            )
            .to_owned(),
        )
    }

    /// Ritka sor az AGENTNEK, hogy szóljon a felhasználónak (csak nem döntött / „most nem” állapotban, 14 naponként
    /// legfeljebb egyszer, összesen legfeljebb 3-szor, legalább 50 rögzített hívás után); megjelenéskor számolja.
    pub(crate) fn agent_hint(&self, now: u64) -> Option<String> {
        let mut consent = self.consent();
        if !self.env.allowed() || !consent.open() || consent.hint_count >= MAX_HINTS {
            return None;
        }
        if consent
            .hint_at
            .is_some_and(|t| now.saturating_sub(t) < REMIND_AFTER)
        {
            return None;
        }
        let today = Day::from_secs(now);
        let calls = self
            .store()
            .totals(today.minus(crate::stats::store::RETENTION_DAYS), today)
            .calls();
        if calls < MIN_CALLS {
            return None;
        }
        consent.hint_count += 1;
        consent.hint_at = Some(now);
        self.save(&consent).ok()?;
        Some(hint_text())
    }
}

/// Az agentnek szóló sor szövege.
pub(crate) fn hint_text() -> String {
    crate::t!(
        "ℹ for the user: sharing anonymous usage counts helps improve callgraph — the user can review and turn it on with: callgraph.sh telemetry preview / telemetry on (do not turn it on yourself)",
        "ℹ a felhasználónak: a névtelen használati számok megosztása segít jobbá tenni a callgraph-ot — a felhasználó megnézheti és bekapcsolhatja: callgraph.sh telemetry preview / telemetry on (magadtól ne kapcsold be)"
    )
    .to_owned()
}

/// A döntés visszaigazolása.
pub(crate) fn decided_line(choice: Choice) -> &'static str {
    match choice {
        Choice::Yes => crate::t!(
            "Telemetry on: closed days are sent once a day in the background. Off: callgraph.sh telemetry off",
            "Telemetria bekapcsolva: a lezárt napok naponta egyszer, a háttérben mennek el. Kikapcsolás: callgraph.sh telemetry off"
        ),
        Choice::Later => crate::t!(
            "Not now — asking once more later. Decide any time: callgraph.sh telemetry on|off",
            "Most nem — később még egyszer rákérdez. Bármikor dönthetsz: callgraph.sh telemetry on|off"
        ),
        Choice::Never | Choice::Off => crate::t!(
            "Telemetry off — nothing is sent, and it will not ask again.",
            "Telemetria kikapcsolva — semmi nem megy el, és többet nem kérdez."
        ),
    }
}
