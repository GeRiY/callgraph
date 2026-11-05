//! Honnan derül ki a legutóbbi kiadás: a `ReleaseSource` trait és megvalósításai.
//!
//! Alapértelmezés: a `https://github.com/GeRiY/callgraph/releases/latest` webes átirányítása (követés nélkül:
//! `302 Location: …/releases/tag/vX.Y.Z`) — nincs óránkénti 60-as API-korlát. Ha az átirányítás nem működik, a
//! tartalék a GitHub API JSON-ja. A `CALLGRAPH_UPDATE_API` (tükör, teszt-fixture, `file://` is) API-formátumú forrás.

use super::fetch::{API_LIMIT, FetchError, Fetcher};
use super::platform::Platform;
use super::release::{Asset, DEFAULT_API, Release, ReleaseError};

/// A legutóbbi kiadás webes címe — átirányít a legutóbbi stabil kiadás tagjére.
pub(crate) const DEFAULT_LATEST: &str = "https://github.com/GeRiY/callgraph/releases/latest";

/// A lekérdezés hibája.
#[derive(Debug, PartialEq)]
pub(crate) enum SourceError {
    /// Letöltési / hálózati hiba.
    Fetch(FetchError),
    /// A válasz nem használható kiadás.
    Release(ReleaseError),
}

/// A legutóbbi stabil kiadás forrása (DI: a parancs és a háttér-ellenőrzés ezt kapja).
pub(crate) trait ReleaseSource {
    /// A lekérdezett cím (az üzenetekhez).
    fn url(&self) -> &str;

    /// A legutóbbi stabil kiadás; `None`, ha még nincs kiadás (404).
    fn latest(&self, fetcher: &dyn Fetcher) -> Result<Option<Release>, SourceError>;
}

/// A környezet szerinti forrás: `CALLGRAPH_UPDATE_API` › átirányítás (tartalék: az API).
pub(crate) fn default_source(platform: &Platform) -> Box<dyn ReleaseSource> {
    match std::env::var("CALLGRAPH_UPDATE_API") {
        Ok(api) if !api.is_empty() => Box::new(ApiSource::new(api)),
        _ => Box::new(FallbackSource {
            primary: Box::new(RedirectSource::new(DEFAULT_LATEST, platform)),
            fallback: Box::new(ApiSource::new(DEFAULT_API)),
        }),
    }
}

/// A `releases/latest` átirányításából olvassa ki a tagot; az asset címe: `…/releases/download/<tag>/<asset>`.
pub(crate) struct RedirectSource {
    url: String,
    asset: String,
}

impl RedirectSource {
    pub(crate) fn new(url: impl Into<String>, platform: &Platform) -> Self {
        Self {
            url: url.into(),
            asset: platform.asset_name(),
        }
    }

    /// A `Location` értelmezése; ha nem egy tagre mutat (pl. `…/releases`), még nincs kiadás.
    fn parse_location(&self, location: &str) -> Result<Option<Release>, SourceError> {
        let location = absolute(&self.url, location);
        let Some((repo, rest)) = location.split_once("/releases/tag/") else {
            return Ok(None);
        };
        let tag = rest.split(['?', '#', '/']).next().unwrap_or_default();
        let assets = vec![Asset {
            name: self.asset.clone(),
            browser_download_url: format!("{repo}/releases/download/{tag}/{}", self.asset),
        }];
        Release::stable(tag.to_owned(), assets)
            .map(Some)
            .map_err(SourceError::Release)
    }
}

/// A relatív `Location` (`/GeRiY/…`) kiegészítése a kérés sémájával és hosztjával.
fn absolute(base: &str, location: &str) -> String {
    if !location.starts_with('/') {
        return location.to_owned();
    }
    let host_end = base
        .find("://")
        .and_then(|i| base[i + 3..].find('/').map(|j| i + 3 + j))
        .unwrap_or(base.len());
    format!("{}{location}", &base[..host_end])
}

impl ReleaseSource for RedirectSource {
    fn url(&self) -> &str {
        &self.url
    }

    fn latest(&self, fetcher: &dyn Fetcher) -> Result<Option<Release>, SourceError> {
        match fetcher.redirect(&self.url) {
            Ok(location) => self.parse_location(&location),
            Err(FetchError::NotFound) => Ok(None),
            Err(e) => Err(SourceError::Fetch(e)),
        }
    }
}

/// A GitHub Releases API JSON-ja (`releases/latest`), vagy egy ilyen formájú fájl / tükör.
pub(crate) struct ApiSource {
    url: String,
}

impl ApiSource {
    pub(crate) fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

impl ReleaseSource for ApiSource {
    fn url(&self) -> &str {
        &self.url
    }

    fn latest(&self, fetcher: &dyn Fetcher) -> Result<Option<Release>, SourceError> {
        let body = match fetcher.get(&self.url, API_LIMIT) {
            Ok(body) => body,
            Err(FetchError::NotFound) => return Ok(None),
            Err(e) => return Err(SourceError::Fetch(e)),
        };
        Release::from_json(&body)
            .map(Some)
            .map_err(SourceError::Release)
    }
}

/// Elsődleges forrás, letöltési hibánál tartalékkal; ha az is hibázik, az elsődleges hibája marad.
pub(crate) struct FallbackSource {
    pub(crate) primary: Box<dyn ReleaseSource>,
    pub(crate) fallback: Box<dyn ReleaseSource>,
}

impl ReleaseSource for FallbackSource {
    fn url(&self) -> &str {
        self.primary.url()
    }

    fn latest(&self, fetcher: &dyn Fetcher) -> Result<Option<Release>, SourceError> {
        match self.primary.latest(fetcher) {
            Err(SourceError::Fetch(e)) => self
                .fallback
                .latest(fetcher)
                .map_err(|_| SourceError::Fetch(e)),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const LATEST: &str = "https://github.com/GeRiY/callgraph/releases/latest";

    /// Hálózat nélküli fetcher: URL → átirányítás-cél / törzs; ami nincs benne, az 404.
    #[derive(Default)]
    struct Fixture {
        redirects: HashMap<String, Result<String, FetchError>>,
        bodies: HashMap<String, Result<Vec<u8>, FetchError>>,
    }

    impl Fetcher for Fixture {
        fn get(&self, url: &str, _limit: u64) -> Result<Vec<u8>, FetchError> {
            self.bodies
                .get(url)
                .cloned()
                .unwrap_or(Err(FetchError::NotFound))
        }
        fn redirect(&self, url: &str) -> Result<String, FetchError> {
            self.redirects
                .get(url)
                .cloned()
                .unwrap_or(Err(FetchError::NotFound))
        }
    }

    fn redirect_to(location: &str) -> Fixture {
        let mut f = Fixture::default();
        f.redirects.insert(LATEST.into(), Ok(location.into()));
        f
    }

    fn source() -> RedirectSource {
        RedirectSource::new(LATEST, &Platform::from_consts("linux", "x86_64"))
    }

    #[test]
    fn reads_the_tag_from_the_redirect_and_builds_the_asset_url() {
        for location in [
            "https://github.com/GeRiY/callgraph/releases/tag/v0.2.0",
            "/GeRiY/callgraph/releases/tag/v0.2.0",
        ] {
            let r = source().latest(&redirect_to(location)).unwrap().unwrap();
            assert_eq!(r.tag, "v0.2.0");
            assert_eq!(r.version.to_string(), "0.2.0");
            assert_eq!(
                r.asset("callgraph-linux-x86_64.zip")
                    .unwrap()
                    .browser_download_url,
                "https://github.com/GeRiY/callgraph/releases/download/v0.2.0/callgraph-linux-x86_64.zip"
            );
        }
    }

    #[test]
    fn no_release_bad_tags_and_fallback() {
        // nincs kiadás: 404, vagy átirányítás a kiadás-listára
        assert_eq!(source().latest(&Fixture::default()), Ok(None));
        let to_list = redirect_to("https://github.com/GeRiY/callgraph/releases");
        assert_eq!(source().latest(&to_list), Ok(None));
        let rc = redirect_to("https://github.com/GeRiY/callgraph/releases/tag/v1.0.0-rc.1");
        assert!(matches!(
            source().latest(&rc),
            Err(SourceError::Release(ReleaseError::NotStable(_)))
        ));

        // az átirányítás hibázik → az API a tartalék; ha az is, az elsődleges hiba marad
        let mut f = Fixture::default();
        f.redirects
            .insert(LATEST.into(), Err(FetchError::Status(429)));
        f.bodies.insert(
            "https://api.invalid/latest".into(),
            Err(FetchError::Status(403)),
        );
        let chain = FallbackSource {
            primary: Box::new(source()),
            fallback: Box::new(ApiSource::new("https://api.invalid/latest")),
        };
        assert_eq!(
            chain.latest(&f),
            Err(SourceError::Fetch(FetchError::Status(429)))
        );
        f.bodies.insert(
            "https://api.invalid/latest".into(),
            Ok(br#"{"tag_name": "v0.3.0", "assets": []}"#.to_vec()),
        );
        let r = chain.latest(&f).unwrap().unwrap();
        assert_eq!(r.version.to_string(), "0.3.0");
        assert_eq!(chain.url(), LATEST);
    }
}
