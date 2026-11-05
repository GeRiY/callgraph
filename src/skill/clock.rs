//! Azonosító és UTC-időbélyeg külső crate nélkül (a bash `openssl rand -hex 4` és `date -u` párja).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 8 hexa-jegyű azonosító (folyamatonként véletlen magú hash: idő + pid + számláló).
pub(crate) fn new_id() -> String {
    let mut h = RandomState::new().build_hasher();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    h.write_u128(nanos);
    h.write_u32(std::process::id());
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    format!("{:08x}", h.finish() as u32)
}

/// `2026-11-05T07:48:00Z` alakú, másodperc-pontos UTC-idő.
pub(crate) fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    iso_from_secs(secs)
}

/// Unix-másodperc → ISO-8601 UTC (Hinnant-féle polgári naptár-algoritmus).
pub(crate) fn iso_from_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
