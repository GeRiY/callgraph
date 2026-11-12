//! Névtelen napi használat-számlálás és a frissítés / letöltés saját címei (check.callgraph.eu, update.callgraph.eu).
//!
//! - **Napi check**: egy telepítés UTC-naponként egyszer, az aznapi első valódi használatkor (egy `callgraph.sh`
//!   parancs vagy egy MCP-eszközhívás) küld egy `GET /v1/check?v=&p=&via=&n=&w=&m=` kérést. Azonosító nincs: csak a
//!   verzió, a platform, a hívás helye és három igen/nem jelző, amit a helyi állapotfájlból (`usage-check.json`,
//!   benne csak az utolsó check napja) számolunk — az állapotfájl soha nem kerül elküldésre.
//! - **Frissítés**: a legutóbbi kiadás forrásláncának első eleme `update.callgraph.eu/v1/latest` (ugyanolyan
//!   `Location`-t ad, mint a GitHub), a zip letöltése `update.callgraph.eu/v1/download/<tag>/<asset>`; hiba esetén
//!   közvetlenül a GitHub. A `SHA256SUMS` mindig közvetlenül a GitHubról jön.
//!
//! Kikapcsolás: `CALLGRAPH_COUNT=0|off|false|no` (a `CALLGRAPH_UPDATE_CHECK=0` és a `CALLGRAPH_TELEMETRY=0` is),
//! `DO_NOT_TRACK=1` (bármely nem üres, nem `0` érték), és `CI` mellett automatikusan. A napi check csak skill- vagy MCPB-telepítésből fut (fejlesztői build és teszt nem küld).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::fetch::{BACKGROUND_TIMEOUT, Fetcher, HttpFetcher};
use super::platform::Platform;
use super::target::InstallTarget;
use super::version::Version;
use crate::skill::SkillHome;

/// A napi check címe.
pub(crate) const CHECK_URL: &str = "https://check.callgraph.eu/v1/check";
/// A frissítés-szolgáltatás alapcíme (`/latest`, `/download/<tag>/<asset>`).
pub(crate) const UPDATE_BASE: &str = "https://update.callgraph.eu/v1";
/// A kiadások közvetlen GitHub-címe (a tartalék letöltéshez).
const GITHUB_RELEASES: &str = "https://github.com/GeRiY/callgraph/releases";
/// A helyi állapotfájl neve a skill- / MCPB-mappában.
pub(crate) const STATE_FILE: &str = "usage-check.json";
/// A háttér-gyerek argumentumai (a query-t a harmadik kapja).
pub(crate) const BACKGROUND_COMMAND: &str = "usage-check";

/// Honnan jön a használat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    Cli,
    Mcp,
}

impl Via {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Mcp => "mcp",
        }
    }
}

/// A napi check jelzői: új telepítés, első a héten, első a hónapban.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Flags {
    pub(crate) new: bool,
    pub(crate) week: bool,
    pub(crate) month: bool,
}

/// Ki van-e kapcsolva a számlálás a környezet szerint (`CALLGRAPH_COUNT`, `DO_NOT_TRACK`, `CI`).
pub(crate) fn opted_out(get: impl Fn(&str) -> Option<String>) -> bool {
    let off = |v: &str| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        )
    };
    // a CALLGRAPH_UPDATE_CHECK=0 eddig is „semmi hálózat” volt, a CALLGRAPH_TELEMETRY=0 a vészkapcsoló: mindkettő
    // a számlálást is kikapcsolja
    let count_off = [
        "CALLGRAPH_COUNT",
        "CALLGRAPH_UPDATE_CHECK",
        "CALLGRAPH_TELEMETRY",
    ]
    .iter()
    .any(|k| get(k).is_some_and(|v| off(&v)));
    let dnt = get("DO_NOT_TRACK").is_some_and(|v| !v.trim().is_empty() && v.trim() != "0");
    let ci = get("CI").is_some_and(|v| !v.trim().is_empty());
    count_off || dnt || ci
}

/// A folyamat környezete szerint ki van-e kapcsolva.
pub(crate) fn opted_out_env() -> bool {
    opted_out(|k| std::env::var(k).ok())
}

/// UTC-nap sorszáma 1970-01-01 óta.
pub(crate) fn day_number(unix_secs: u64) -> i64 {
    (unix_secs / 86_400) as i64
}

/// A nap sorszámából (év, hónap, nap) — Howard Hinnant `civil_from_days` algoritmusa.
pub(crate) fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// `YYYY-MM-DD` → a nap sorszáma (az állapotfájlhoz); érvénytelen szövegre `None`.
pub(crate) fn parse_day(text: &str) -> Option<i64> {
    let mut parts = text.trim().splitn(3, '-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // days_from_civil (Howard Hinnant); a visszaalakítás kiszűri a nem létező napot (pl. 02-30)
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    (civil(days) == (year, month, day)).then_some(days)
}

/// A nap sorszáma `YYYY-MM-DD` alakban.
pub(crate) fn format_day(days: i64) -> String {
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Az ISO-hét sorszáma (hétfőtől vasárnapig; 1970-01-01 csütörtök volt).
fn week_index(days: i64) -> i64 {
    (days + 3).div_euclid(7)
}

/// A hónap sorszáma (év·12 + hónap).
fn month_index(days: i64) -> i64 {
    let (y, m, _) = civil(days);
    y * 12 + i64::from(m)
}

/// Kell-e ma checket küldeni, és milyen jelzőkkel: csak ha az utolsó check nem ma volt (a visszaállított óra is
/// új napnak számít, a jelzők ilyenkor is a naptár szerint jönnek).
pub(crate) fn decide(last: Option<i64>, today: i64) -> Option<Flags> {
    if last == Some(today) {
        return None;
    }
    Some(Flags {
        new: last.is_none(),
        week: last.is_none_or(|l| week_index(l) != week_index(today)),
        month: last.is_none_or(|l| month_index(l) != month_index(today)),
    })
}

/// A check query-je.
pub(crate) fn check_query(version: &str, platform: &str, via: Via, flags: Flags) -> String {
    format!(
        "v={version}&p={platform}&via={}&n={}&w={}&m={}",
        via.as_str(),
        u8::from(flags.new),
        u8::from(flags.week),
        u8::from(flags.month)
    )
}

/// Az utolsó check napja az állapotfájlból (hiányzó / sérült fájl: `None`).
pub(crate) fn load_last(file: &Path) -> Option<i64> {
    let text = std::fs::read_to_string(file).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    parse_day(value.get("last_day")?.as_str()?)
}

/// Az utolsó check napjának mentése (átmeneti fájlon át, hogy félbeszakadt írás ne maradjon).
pub(crate) fn save_last(file: &Path, day: i64) -> std::io::Result<()> {
    let body = serde_json::json!({ "last_day": format_day(day) }).to_string();
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, file)
}

/// A napi check elküldése (DI: a CLI leválasztott gyerekfolyamatot indít, az MCP háttérszálat, a teszt rögzít).
pub(crate) trait CheckSender: Send + Sync {
    fn send(&self, query: String);
}

/// CLI: leválasztott gyerekfolyamat (`callgraph usage-check --background <query>`), hogy a parancs ne várjon.
pub(crate) struct DetachedSender;

impl CheckSender for DetachedSender {
    fn send(&self, query: String) {
        super::check::spawn_detached([BACKGROUND_COMMAND, "--background", query.as_str()]);
    }
}

/// MCP: háttérszál (a szerver hosszan fut), rövid időkorláttal.
pub(crate) struct ThreadSender;

impl CheckSender for ThreadSender {
    fn send(&self, query: String) {
        let _ = std::thread::Builder::new()
            .name("usage-check".into())
            .spawn(move || send_now(&HttpFetcher::new(BACKGROUND_TIMEOUT), &query));
    }
}

/// A kérés tényleges elküldése; minden hibát elnyel (nincs újrapróbálás: a nap már elkönyvelve).
pub(crate) fn send_now(fetcher: &dyn Fetcher, query: &str) {
    let _ = fetcher.get(&format!("{CHECK_URL}?{query}"), 1024);
}

/// A napi check a függőségeivel. A forró úton csak egy kis fájl olvasása / írása történik, naponta egyszer.
pub(crate) struct UsageCheck {
    file: Option<PathBuf>,
    version: String,
    platform: String,
    sender: Box<dyn CheckSender>,
    /// Az utoljára ismert check-nap (az MCP-szerver ne olvassa a fájlt minden hívásnál).
    last: Mutex<Option<Option<i64>>>,
}

impl UsageCheck {
    /// A valódi környezet; kikapcsolva (`file: None`), ha a környezet tiltja, vagy nem skill- / MCPB-telepítésből fut.
    pub(crate) fn from_env(home: &SkillHome, sender: Box<dyn CheckSender>) -> Self {
        let installed = InstallTarget::current().is_some() || is_bundle(home.dir());
        let file = (installed && !opted_out_env()).then(|| home.dir().join(STATE_FILE));
        Self::new(
            file,
            Version::current().to_string(),
            Platform::current().dir(),
            sender,
        )
    }

    pub(crate) fn new(
        file: Option<PathBuf>,
        version: String,
        platform: String,
        sender: Box<dyn CheckSender>,
    ) -> Self {
        Self {
            file,
            version,
            platform,
            sender,
            last: Mutex::new(None),
        }
    }

    /// Kikapcsolt példány (tesztek, beágyazott szerver).
    pub(crate) fn disabled() -> Self {
        Self::new(None, String::new(), String::new(), Box::new(NoSender))
    }

    /// Egy valódi használat után: ha ma még nem volt check, elkönyveli a napot és elküldi. Hibát nem ad.
    pub(crate) fn after_use(&self, now: u64, via: Via) {
        let Some(file) = &self.file else {
            return;
        };
        let today = day_number(now);
        let Ok(mut cached) = self.last.lock() else {
            return;
        };
        if *cached == Some(Some(today)) {
            return;
        }
        let last = load_last(file);
        *cached = Some(last);
        let Some(flags) = decide(last, today) else {
            return;
        };
        // előbb a nap elkönyvelése: párhuzamos futások ne küldjenek még egyet, és mentés nélkül (írásvédett mappa)
        // ne küldjünk minden futáskor
        if save_last(file, today).is_err() {
            return;
        }
        *cached = Some(Some(today));
        self.sender
            .send(check_query(&self.version, &self.platform, via, flags));
    }
}

struct NoSender;

impl CheckSender for NoSender {
    fn send(&self, _query: String) {}
}

/// MCPB-csomag gyökere-e a mappa (a manifest.json mellett bin/ van).
fn is_bundle(dir: &Path) -> bool {
    dir.join("manifest.json").is_file() && dir.join("bin").is_dir()
}

/// A frissítés-ellenőrzés saját címe (a forráslánc első eleme).
pub(crate) fn latest_url(version: &str, platform: &str) -> String {
    format!("{UPDATE_BASE}/latest?v={version}&p={platform}")
}

/// A frissítés letöltésének saját címe.
pub(crate) fn download_url(tag: &str, asset: &str, version: &str) -> String {
    format!("{UPDATE_BASE}/download/{tag}/{asset}?k=update&v={version}")
}

/// A saját letöltési cím közvetlen GitHub-megfelelője (tartalék, ha a saját cím hibázik); más címre `None`.
pub(crate) fn direct_download_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix(UPDATE_BASE)?.strip_prefix("/download/")?;
    let path = rest.split(['?', '#']).next()?;
    let (tag, asset) = path.split_once('/')?;
    if tag.is_empty() || asset.is_empty() || asset.contains('/') {
        return None;
    }
    Some(if tag == "latest" {
        format!("{GITHUB_RELEASES}/latest/download/{asset}")
    } else {
        format!("{GITHUB_RELEASES}/download/{tag}/{asset}")
    })
}

#[cfg(test)]
mod tests;
