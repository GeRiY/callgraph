//! A GitHub Releases API (`/repos/<owner>/<repo>/releases/latest`) válaszának a frissítéshez szükséges része.

use serde::Deserialize;

use super::version::Version;

/// A legutóbbi kiadás lekérdezésének alapértelmezett címe (felülírható: `CALLGRAPH_UPDATE_API`).
pub(crate) const DEFAULT_API: &str = "https://api.github.com/repos/GeRiY/callgraph/releases/latest";

/// Egy release-asset: a fájlnév és a letöltési cím.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Asset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// Egy kiadás: a tagből értelmezett verzió és az assetek.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Release {
    pub(crate) tag: String,
    pub(crate) version: Version,
    pub(crate) assets: Vec<Asset>,
}

/// Miért nem használható a válasz.
#[derive(Debug, PartialEq)]
pub(crate) enum ReleaseError {
    /// Nem értelmezhető JSON (vagy hiányzik a `tag_name`).
    Json(String),
    /// A tag nem semver (`v1.2.3`).
    BadTag(String),
    /// Vázlat vagy előzetes kiadás — nem frissítünk rá.
    NotStable(String),
}

impl Release {
    /// A `releases/latest` JSON-válasz értelmezése; a vázlat / pre-release és a nem semver tag hiba.
    pub(crate) fn from_json(body: &[u8]) -> Result<Self, ReleaseError> {
        let raw: RawRelease =
            serde_json::from_slice(body).map_err(|e| ReleaseError::Json(e.to_string()))?;
        let version = Version::parse(&raw.tag_name)
            .ok_or_else(|| ReleaseError::BadTag(raw.tag_name.clone()))?;
        if raw.draft || raw.prerelease || version.is_prerelease() {
            return Err(ReleaseError::NotStable(raw.tag_name));
        }
        Ok(Self {
            tag: raw.tag_name,
            version,
            assets: raw.assets,
        })
    }

    /// A megadott nevű asset (pl. `callgraph-linux-x86_64.zip`).
    pub(crate) fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "url": "https://api.github.com/repos/GeRiY/callgraph/releases/1",
        "tag_name": "v0.2.0",
        "name": "v0.2.0",
        "draft": false,
        "prerelease": false,
        "assets": [
            {"name": "callgraph-0.2.0.tar.gz", "size": 10,
             "browser_download_url": "https://github.com/GeRiY/callgraph/releases/download/v0.2.0/callgraph-0.2.0.tar.gz"},
            {"name": "callgraph-linux-x86_64.zip", "size": 20,
             "browser_download_url": "https://github.com/GeRiY/callgraph/releases/download/v0.2.0/callgraph-linux-x86_64.zip"}
        ]
    }"#;

    #[test]
    fn parses_latest_release_fixture() {
        let r = Release::from_json(FIXTURE.as_bytes()).unwrap();
        assert_eq!(r.tag, "v0.2.0");
        assert_eq!(r.version.to_string(), "0.2.0");
        assert_eq!(
            r.asset("callgraph-linux-x86_64.zip")
                .unwrap()
                .browser_download_url,
            "https://github.com/GeRiY/callgraph/releases/download/v0.2.0/callgraph-linux-x86_64.zip"
        );
        assert!(r.asset("callgraph-windows-x86_64.zip").is_none());
    }

    #[test]
    fn rejects_unstable_and_malformed_releases() {
        let pre = FIXTURE.replace(r#""prerelease": false"#, r#""prerelease": true"#);
        assert!(matches!(
            Release::from_json(pre.as_bytes()),
            Err(ReleaseError::NotStable(_))
        ));
        let rc = FIXTURE.replace(
            "v0.2.0\",\n        \"name",
            "v0.3.0-rc.1\",\n        \"name",
        );
        assert!(matches!(
            Release::from_json(rc.as_bytes()),
            Err(ReleaseError::NotStable(_))
        ));
        let bad = FIXTURE.replace("\"tag_name\": \"v0.2.0\"", "\"tag_name\": \"nightly\"");
        assert_eq!(
            Release::from_json(bad.as_bytes()),
            Err(ReleaseError::BadTag("nightly".into()))
        );
        assert!(matches!(
            Release::from_json(b"{\"message\": \"Not Found\"}"),
            Err(ReleaseError::Json(_))
        ));
    }
}
