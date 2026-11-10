//! Egy beküldés: zárfájl (`telemetry.lock`: egyszerre csak egy folyamat küld), az állapot újraolvasása, a törzs,
//! egy POST 5 s időkorláttal, és csak HTTP 2xx után a napok beküldöttnek jelölése. Újrapróbálás nincs (a következő
//! futás próbálja, legkorábban egy óra múlva).

use std::fs::{OpenOptions, TryLockError};
use std::time::Duration;

use super::{SendOutcome, Telemetry, payload};
use crate::stats::day::Day;

/// A HTTP-küldő (DI: a tesztek helyi szervert vagy hamisítványt adnak).
pub(crate) trait Sender {
    /// A válasz HTTP-státusza, vagy a hálózati hiba szövege.
    fn post(&self, url: &str, body: &str) -> Result<u16, String>;
}

/// Az éles küldő: ureq (rustls), 5 s időkorlát, átirányítás nélkül.
pub(crate) struct HttpSender;

/// A teljes kérés időkorlátja.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_FILE: &str = "telemetry.lock";

impl Sender for HttpSender {
    fn post(&self, url: &str, body: &str) -> Result<u16, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent("callgraph-telemetry")
            .build()
            .into();
        agent
            .post(url)
            .header("Content-Type", "application/json")
            .send(body)
            .map(|response| response.status().as_u16())
            .map_err(|e| e.to_string())
    }
}

pub(super) fn send_now(telemetry: &Telemetry, sender: &dyn Sender, now: u64) -> SendOutcome {
    if !telemetry.env.allowed() || std::fs::create_dir_all(&telemetry.dir).is_err() {
        return SendOutcome::Skipped;
    }
    let Ok(lock) = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(telemetry.dir.join(LOCK_FILE))
    else {
        return SendOutcome::Skipped;
    };
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return SendOutcome::Skipped,
        Err(TryLockError::Error(_)) => {}
    }
    let mut consent = telemetry.consent();
    if !telemetry.due(&consent, now) {
        return SendOutcome::Skipped;
    }
    let days = telemetry.pending(&consent, Day::from_secs(now));
    let body = match serde_json::to_string(&payload::build(&telemetry.store(), &days)) {
        Ok(body) => body,
        Err(e) => return SendOutcome::Failed(e.to_string()),
    };
    consent.last_attempt = Some(now);
    let _ = telemetry.save(&consent);
    match sender.post(&telemetry.env.url, &body) {
        Ok(status) if (200..300).contains(&status) => {
            consent.last_sent_day = days.last().map(ToString::to_string);
            consent.last_send = Some(now);
            let _ = telemetry.save(&consent);
            SendOutcome::Sent(days.len())
        }
        Ok(status) => SendOutcome::Failed(format!("HTTP {status}")),
        Err(e) => SendOutcome::Failed(e),
    }
}
