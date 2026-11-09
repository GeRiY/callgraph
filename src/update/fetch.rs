//! Letöltés a frissítéshez: a `Fetcher` trait (a parancs ezt kapja — a tesztek hálózat nélkül, hamis fetcherrel futnak)
//! és a valódi megvalósítás: HTTPS `ureq`-kel (rustls), `file://` / helyi út fájlból (tükör, teszt, `--from`).
//! A `redirect` egy átirányítás célja (követés nélkül) — így derül ki a legutóbbi kiadás a GitHub API nélkül.

use std::time::Duration;

/// A frissítő `User-Agent` fejléce — általános, személyes adat nélkül (a GitHub API megköveteli).
const USER_AGENT: &str = "callgraph-updater";

/// A JSON-válasz felső mérete.
pub(crate) const API_LIMIT: u64 = 4 * 1024 * 1024;

/// A letöltött zip felső mérete.
pub(crate) const ASSET_LIMIT: u64 = 256 * 1024 * 1024;

/// Egy letöltés hibája.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FetchError {
    /// HTTP 404 (vagy nem létező helyi fájl) — pl. még nincs kiadás.
    NotFound,
    /// Más, nem sikeres HTTP-státusz.
    Status(u16),
    /// Hálózati / I/O hiba (szövegesen).
    Other(String),
}

/// Egy cím tartalmának letöltése, legfeljebb `limit` bájt.
pub(crate) trait Fetcher {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, FetchError>;

    /// Mint a `get`, de gyorsítótár nélkül (a kiadás-lekérdezésekhez: a GitHub egy kiadás után percekig a régi
    /// `releases/latest`-et adhatja). Alapból a `get`; a valódi fetcher a címhez egy egyedi query-paramétert fűz.
    fn get_fresh(&self, url: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
        self.get(url, limit)
    }

    /// Egy átirányító cím célja (`Location`), az átirányítás követése nélkül; ha nem irányít át, hiba.
    /// A valódi fetcher gyorsítótár nélkül kérdez (mint a `get_fresh`).
    fn redirect(&self, url: &str) -> Result<String, FetchError>;
}

/// Gyorsítótár-kerülő cím: `?cb=<unix-idő>` (vagy `&cb=…`); a GitHub az ismeretlen paramétert figyelmen kívül
/// hagyja, a köztes gyorsítótár viszont új címnek látja.
pub(crate) fn cache_busted(url: &str, now: u64) -> String {
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}cb={now}")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Az előtér (`self-update`) időkorlátja: a zip letöltése is belefér.
pub(crate) const FOREGROUND_TIMEOUT: Duration = Duration::from_secs(300);
/// A háttér-ellenőrzés időkorlátja: rövid, hogy a gyerekfolyamat hamar kilépjen.
pub(crate) const BACKGROUND_TIMEOUT: Duration = Duration::from_secs(5);
/// Az MCP-szerver `--auto-update` lépésének kérésenkénti időkorlátja (a zip letöltése is): az indítást legfeljebb
/// ennyivel késlelteti kérésenként.
pub(crate) const AUTO_UPDATE_TIMEOUT: Duration = Duration::from_secs(30);
/// Az `--auto-update` kapcsolódási időkorlátja: hálózat nélkül gyorsan feladja.
pub(crate) const AUTO_UPDATE_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// A valódi fetcher: `http(s)://` → ureq, `file://<út>` vagy sima út → a fájl tartalma.
pub(crate) struct HttpFetcher {
    agent: ureq::Agent,
    /// Ugyanez, de az átirányításokat nem követi (a `redirect`-hez).
    no_follow: ureq::Agent,
}

impl HttpFetcher {
    /// Fetcher a megadott teljes időkorláttal (a kapcsolódásé legfeljebb 20 s).
    pub(crate) fn new(timeout: Duration) -> Self {
        Self::with_connect(timeout, timeout.min(Duration::from_secs(20)))
    }

    /// Fetcher külön teljes (kérésenkénti) és kapcsolódási időkorláttal.
    pub(crate) fn with_connect(timeout: Duration, connect: Duration) -> Self {
        let agent = |redirects: u32| -> ureq::Agent {
            ureq::Agent::config_builder()
                .timeout_global(Some(timeout))
                .timeout_connect(Some(connect))
                .max_redirects(redirects)
                .user_agent(USER_AGENT)
                .build()
                .into()
        };
        Self {
            agent: agent(10),
            no_follow: agent(0),
        }
    }

    fn get_http(&self, url: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
        let mut response = self
            .agent
            .get(url)
            .header(
                "Accept",
                "application/vnd.github+json, application/octet-stream",
            )
            .header("Cache-Control", "no-cache")
            .call()
            .map_err(http_error)?;
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|e| FetchError::Other(e.to_string()))
    }
}

fn http_error(e: ureq::Error) -> FetchError {
    match e {
        ureq::Error::StatusCode(404) => FetchError::NotFound,
        ureq::Error::StatusCode(code) => FetchError::Status(code),
        other => FetchError::Other(other.to_string()),
    }
}

fn is_http(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

impl Fetcher for HttpFetcher {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
        if is_http(url) {
            return self.get_http(url, limit);
        }
        read_local(url.strip_prefix("file://").unwrap_or(url), limit)
    }

    fn get_fresh(&self, url: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
        if is_http(url) {
            return self.get_http(&cache_busted(url, now_secs()), limit);
        }
        self.get(url, limit)
    }

    fn redirect(&self, url: &str) -> Result<String, FetchError> {
        if !is_http(url) {
            return Err(FetchError::Other(format!("{url}: not an http(s) URL")));
        }
        let response = self
            .no_follow
            .get(&cache_busted(url, now_secs()))
            .header("Cache-Control", "no-cache")
            .call()
            .map_err(http_error)?;
        let status = response.status();
        if !status.is_redirection() {
            return Err(FetchError::Other(format!(
                "{url}: no redirect (HTTP {})",
                status.as_u16()
            )));
        }
        response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| FetchError::Other(format!("{url}: redirect without Location")))
    }
}

fn read_local(path: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
    let bytes = std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FetchError::NotFound,
        _ => FetchError::Other(format!("{path}: {e}")),
    })?;
    if bytes.len() as u64 > limit {
        return Err(FetchError::Other(format!("{path}: > {limit} bytes")));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_file_urls_and_plain_paths_without_network() {
        let dir = std::env::temp_dir().join(format!("callgraph-fetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("latest.json");
        std::fs::write(&file, b"{}").unwrap();
        let f = HttpFetcher::new(BACKGROUND_TIMEOUT);
        let url = format!("file://{}", file.display());
        assert_eq!(f.get(&url, 10).unwrap(), b"{}");
        assert_eq!(f.get(&file.display().to_string(), 10).unwrap(), b"{}");
        assert!(matches!(f.get(&url, 1), Err(FetchError::Other(_))));
        let missing = dir.join("missing.json").display().to_string();
        assert_eq!(f.get(&missing, 10), Err(FetchError::NotFound));
        assert!(matches!(f.redirect(&url), Err(FetchError::Other(_))));
        assert_eq!(f.get_fresh(&url, 10).unwrap(), b"{}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cache_buster_appends_a_query_parameter() {
        assert_eq!(
            cache_busted("https://x.invalid/latest", 7),
            "https://x.invalid/latest?cb=7"
        );
        assert_eq!(
            cache_busted("https://x.invalid/releases?per_page=5", 7),
            "https://x.invalid/releases?per_page=5&cb=7"
        );
    }
}
