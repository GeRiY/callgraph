//! Honnan derül ki a legutóbbi kiadás: a `ReleaseSource` trait és megvalósításai.
//!
//! Alapértelmezés: a `https://github.com/GeRiY/callgraph/releases/latest` webes átirányítása (követés nélkül:
//! `302 Location: …/releases/tag/vX.Y.Z`) — nincs óránkénti 60-as API-korlát. Ha az átirányítás nem működik, a
//! tartalék a GitHub API JSON-ja. A `CALLGRAPH_UPDATE_API` (tükör, teszt-fixture, `file://` is) API-formátumú forrás.

use super::fetch::{API_LIMIT, FetchError, Fetcher};
use super::platform::Platform;
use super::release::{Asset, DEFAULT_API, Release, ReleaseError};
use super::version::Version;

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

/// A környezet szerinti forrás: `CALLGRAPH_UPDATE_API` › (számlálás bekapcsolva: `update.callgraph.eu` ›)
/// GitHub-átirányítás › GitHub API. A saját szerver hibájánál / 503-jánál a lánc a GitHubbal folytatódik.
pub(crate) fn default_source(platform: &Platform) -> Box<dyn ReleaseSource> {
    match std::env::var("CALLGRAPH_UPDATE_API") {
        Ok(api) if !api.is_empty() => Box::new(ApiSource::new(api)),
        _ => source_chain(
            platform,
            !super::usage::opted_out_env(),
            &Version::current().to_string(),
        ),
    }
}

/// A frissítési forráslánc; `counted`: a saját (számláló) szerver az első.
pub(crate) fn source_chain(
    platform: &Platform,
    counted: bool,
    version: &str,
) -> Box<dyn ReleaseSource> {
    let github: Box<dyn ReleaseSource> = Box::new(FallbackSource {
        primary: Box::new(RedirectSource::new(DEFAULT_LATEST, platform)),
        fallback: Box::new(ApiSource::new(DEFAULT_API)),
    });
    if !counted {
        return github;
    }
    Box::new(FallbackSource {
        primary: Box::new(RedirectSource::counted(platform, version)),
        fallback: github,
    })
}

/// A `releases/latest` átirányításából olvassa ki a tagot; az asset címe: `…/releases/download/<tag>/<asset>`.
pub(crate) struct RedirectSource {
    url: String,
    asset: String,
    /// A saját (számláló) szerver: a zip a `update.callgraph.eu/v1/download` címről jön (a saját verzióval).
    counted: Option<String>,
}

impl RedirectSource {
    pub(crate) fn new(url: impl Into<String>, platform: &Platform) -> Self {
        Self {
            url: url.into(),
            asset: platform.asset_name(),
            counted: None,
        }
    }

    /// A saját szerver (`update.callgraph.eu/v1/latest`): ugyanolyan `Location`-t ad, mint a GitHub.
    pub(crate) fn counted(platform: &Platform, version: &str) -> Self {
        Self {
            url: super::usage::latest_url(version, &platform.dir()),
            asset: platform.asset_name(),
            counted: Some(version.to_owned()),
        }
    }

    /// A `Location` értelmezése; ha nem egy tagre mutat (pl. `…/releases`), még nincs kiadás.
    fn parse_location(&self, location: &str) -> Result<Option<Release>, SourceError> {
        let location = absolute(&self.url, location);
        let Some((repo, rest)) = location.split_once("/releases/tag/") else {
            return Ok(None);
        };
        let tag = rest.split(['?', '#', '/']).next().unwrap_or_default();
        let direct = |name: &str| format!("{repo}/releases/download/{tag}/{name}");
        let assets = match &self.counted {
            None => vec![Asset {
                name: self.asset.clone(),
                browser_download_url: direct(&self.asset),
            }],
            // a zip a saját letöltési címről (számolva; hibánál a frissítő a GitHub-címmel próbálja újra),
            // a SHA256SUMS közvetlenül a GitHubról
            Some(version) => vec![
                Asset {
                    name: self.asset.clone(),
                    browser_download_url: super::usage::download_url(tag, &self.asset, version),
                },
                Asset {
                    name: super::checksum::SUMS_FILE.to_owned(),
                    browser_download_url: direct(super::checksum::SUMS_FILE),
                },
            ],
        };
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
        let result = match fetcher.redirect(&self.url) {
            Ok(location) => self.parse_location(&location),
            Err(FetchError::NotFound) => Ok(None),
            Err(e) => Err(SourceError::Fetch(e)),
        };
        // a saját szerver csak egy biztos kiadást adhat: minden más (nincs kiadás, rossz tag, hiba) letöltési
        // hibának számít, hogy a lánc a GitHubbal folytatódjon
        match result {
            Ok(Some(release)) => Ok(Some(release)),
            Err(SourceError::Fetch(e)) => Err(SourceError::Fetch(e)),
            _ if self.counted.is_some() => Err(SourceError::Fetch(FetchError::Other(
                "update.callgraph.eu: no usable release".to_owned(),
            ))),
            other => other,
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
        let body = match fetcher.get_fresh(&self.url, API_LIMIT) {
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

    const OURS: &str = "https://update.callgraph.eu/v1/latest?v=0.9.3&p=linux-x86_64";

    fn chain() -> Box<dyn ReleaseSource> {
        source_chain(&Platform::from_consts("linux", "x86_64"), true, "0.9.3")
    }

    fn api_body(tag: &str) -> Vec<u8> {
        format!(
            r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"assets":[{{"name":"callgraph-linux-x86_64.zip","browser_download_url":"https://github.com/GeRiY/callgraph/releases/download/{tag}/callgraph-linux-x86_64.zip"}}]}}"#
        )
        .into_bytes()
    }

    #[test]
    fn counted_chain_asks_our_server_first_and_downloads_through_it() {
        let mut f = Fixture::default();
        f.redirects.insert(
            OURS.into(),
            Ok("https://github.com/GeRiY/callgraph/releases/tag/v0.9.4".into()),
        );
        let release = chain().latest(&f).unwrap().unwrap();
        assert_eq!(release.tag, "v0.9.4");
        assert_eq!(
            release
                .asset("callgraph-linux-x86_64.zip")
                .unwrap()
                .browser_download_url,
            "https://update.callgraph.eu/v1/download/v0.9.4/callgraph-linux-x86_64.zip?k=update&v=0.9.3"
        );
        // a SHA256SUMS mindig közvetlenül a GitHubról
        assert_eq!(
            release.asset("SHA256SUMS").unwrap().browser_download_url,
            "https://github.com/GeRiY/callgraph/releases/download/v0.9.4/SHA256SUMS"
        );
    }

    #[test]
    fn counted_chain_falls_back_to_github_on_any_trouble() {
        for ours in [
            Err(FetchError::Status(503)),
            Err(FetchError::Other("timeout".into())),
            Err(FetchError::NotFound),
            Ok("https://github.com/GeRiY/callgraph/releases".to_owned()),
            Ok("https://github.com/GeRiY/callgraph/releases/tag/not-a-version".to_owned()),
        ] {
            let mut f = Fixture::default();
            f.redirects.insert(OURS.into(), ours.clone());
            f.redirects.insert(
                LATEST.into(),
                Ok("https://github.com/GeRiY/callgraph/releases/tag/v0.9.4".into()),
            );
            let release = chain()
                .latest(&f)
                .unwrap()
                .unwrap_or_else(|| panic!("{ours:?}"));
            assert_eq!(release.tag, "v0.9.4", "{ours:?}");
            // a GitHub-ágon a letöltés közvetlen
            assert_eq!(
                release
                    .asset("callgraph-linux-x86_64.zip")
                    .unwrap()
                    .browser_download_url,
                "https://github.com/GeRiY/callgraph/releases/download/v0.9.4/callgraph-linux-x86_64.zip"
            );
        }
    }

    #[test]
    fn counted_chain_reaches_the_api_when_both_redirects_fail() {
        let mut f = Fixture::default();
        f.redirects
            .insert(OURS.into(), Err(FetchError::Status(503)));
        f.redirects
            .insert(LATEST.into(), Err(FetchError::Status(500)));
        f.bodies.insert(DEFAULT_API.into(), Ok(api_body("v0.9.4")));
        assert_eq!(chain().latest(&f).unwrap().unwrap().tag, "v0.9.4");
    }

    #[test]
    fn uncounted_chain_never_touches_our_server() {
        let mut f = Fixture::default();
        f.redirects.insert(
            OURS.into(),
            Ok("https://github.com/GeRiY/callgraph/releases/tag/v9.9.9".into()),
        );
        f.redirects.insert(
            LATEST.into(),
            Ok("https://github.com/GeRiY/callgraph/releases/tag/v0.9.4".into()),
        );
        let release = source_chain(&Platform::from_consts("linux", "x86_64"), false, "0.9.3")
            .latest(&f)
            .unwrap()
            .unwrap();
        assert_eq!(release.tag, "v0.9.4");
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
