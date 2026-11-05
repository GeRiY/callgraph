//! Letöltés a frissítéshez: a `Fetcher` trait (a parancs ezt kapja — a tesztek hálózat nélkül, hamis fetcherrel futnak)
//! és a valódi megvalósítás: HTTPS `ureq`-kel (rustls), `file://` / helyi út fájlból (tükör, teszt, `--from`).

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
}

/// A valódi fetcher: `http(s)://` → ureq, `file://<út>` vagy sima út → a fájl tartalma.
pub(crate) struct HttpFetcher {
    agent: ureq::Agent,
}

impl HttpFetcher {
    pub(crate) fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(300)))
            .timeout_connect(Some(Duration::from_secs(20)))
            .user_agent(USER_AGENT)
            .build()
            .into();
        Self { agent }
    }

    fn get_http(&self, url: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
        let mut response = self
            .agent
            .get(url)
            .header(
                "Accept",
                "application/vnd.github+json, application/octet-stream",
            )
            .call()
            .map_err(|e| match e {
                ureq::Error::StatusCode(404) => FetchError::NotFound,
                ureq::Error::StatusCode(code) => FetchError::Status(code),
                other => FetchError::Other(other.to_string()),
            })?;
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|e| FetchError::Other(e.to_string()))
    }
}

impl Fetcher for HttpFetcher {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, FetchError> {
        if url.starts_with("http://") || url.starts_with("https://") {
            return self.get_http(url, limit);
        }
        read_local(url.strip_prefix("file://").unwrap_or(url), limit)
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
        let f = HttpFetcher::new();
        let url = format!("file://{}", file.display());
        assert_eq!(f.get(&url, 10).unwrap(), b"{}");
        assert_eq!(f.get(&file.display().to_string(), 10).unwrap(), b"{}");
        assert!(matches!(f.get(&url, 1), Err(FetchError::Other(_))));
        let missing = dir.join("missing.json").display().to_string();
        assert_eq!(f.get(&missing, 10), Err(FetchError::NotFound));
        let _ = std::fs::remove_dir_all(dir);
    }
}
