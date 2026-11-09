//! Egy kiadás (tag, verzió, assetek) és a GitHub Releases API (`/repos/<owner>/<repo>/releases/latest`)
//! válaszának a frissítéshez szükséges része. Az API csak tartalék / tükör; az alapértelmezett forrás a
//! `releases/latest` webes átirányítása (`update::source`).

use serde::Deserialize;

use super::version::Version;

/// A legutóbbi kiadás az API-ból (tartalék, ha az átirányítás nem működik; a `CALLGRAPH_UPDATE_API` ezt írja felül).
pub(crate) const DEFAULT_API: &str = "https://api.github.com/repos/GeRiY/callgraph/releases/latest";

/// A legutóbbi néhány kiadás listája: a `releases/latest` egy friss kiadás után percekig a régit adhatja — ha az
/// „naprakész”-t mond, a lista első stabil kiadása is számít (`newest_stable`).
pub(crate) const DEFAULT_LIST: &str = "https://api.github.com/repos/GeRiY/callgraph/releases?per_page=5";

/// A dev-csatorna: a develop-ág gördülő előzetes kiadása (`dev` tag; a CI minden develop-pushnál frissíti).
/// A `CALLGRAPH_UPDATE_DEV_API` (tükör, teszt; `file://` is) felülírja.
pub(crate) const DEFAULT_DEV_API: &str =
    "https://api.github.com/repos/GeRiY/callgraph/releases/tags/dev";

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
    /// A kiadás leírása; a dev-kiadásé `version: X.Y.Z-dev.<sha7>` és `date: ÉÉÉÉ-HH-NN` sort tartalmaz.
    #[serde(default)]
    body: Option<String>,
}

/// A dev-csatorna egy buildje: a kiadás (a verzió a leírásból) és a build dátuma (ha a leírásban szerepel).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DevBuild {
    pub(crate) release: Release,
    pub(crate) date: Option<String>,
}

impl DevBuild {
    /// A `releases/tags/dev` JSON-válasz értelmezése: a verzió a leírás `version:` sorából (`X.Y.Z-dev.<sha7>`);
    /// vázlat, hiányzó vagy nem dev-verzió hiba.
    pub(crate) fn from_json(body: &[u8]) -> Result<Self, ReleaseError> {
        let raw: RawRelease =
            serde_json::from_slice(body).map_err(|e| ReleaseError::Json(e.to_string()))?;
        if raw.draft {
            return Err(ReleaseError::NotStable(raw.tag_name));
        }
        let text = raw.body.unwrap_or_default();
        let field = |key: &str| {
            text.lines()
                .find_map(|l| l.trim().strip_prefix(key))
                .map(|v| v.trim().trim_matches('`').to_owned())
        };
        let version = field("version:")
            .and_then(|v| Version::parse(&v))
            .filter(Version::is_dev)
            .ok_or_else(|| ReleaseError::BadTag(raw.tag_name.clone()))?;
        Ok(Self {
            release: Release {
                tag: raw.tag_name,
                version,
                assets: raw.assets,
            },
            date: field("date:").filter(|d| !d.is_empty()),
        })
    }
}

/// A kiadás-lista (`/releases?per_page=N`) legújabb stabil kiadása: a vázlat, a pre-release (a `dev` is) és a nem
/// semver tag kimarad; több stabil közül a legmagasabb verzió. `Ok(None)`: nincs stabil a listában.
pub(crate) fn newest_stable(body: &[u8]) -> Result<Option<Release>, ReleaseError> {
    let list: Vec<RawRelease> =
        serde_json::from_slice(body).map_err(|e| ReleaseError::Json(e.to_string()))?;
    Ok(list
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| Release::stable(r.tag_name, r.assets).ok())
        .max_by(|a, b| a.version.cmp(&b.version)))
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
        if raw.draft || raw.prerelease {
            return Err(ReleaseError::NotStable(raw.tag_name));
        }
        Self::stable(raw.tag_name, raw.assets)
    }

    /// Kiadás egy tagből: a nem semver tag és az előzetes verzió (`v1.0.0-rc.1`) hiba.
    pub(crate) fn stable(tag: String, assets: Vec<Asset>) -> Result<Self, ReleaseError> {
        let version = Version::parse(&tag).ok_or_else(|| ReleaseError::BadTag(tag.clone()))?;
        if version.is_prerelease() {
            return Err(ReleaseError::NotStable(tag));
        }
        Ok(Self {
            tag,
            version,
            assets,
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

    #[test]
    fn picks_the_newest_stable_release_from_the_list() {
        let list = br#"[
            {"tag_name": "dev", "draft": false, "prerelease": true, "assets": []},
            {"tag_name": "v0.6.0", "draft": true, "prerelease": false, "assets": []},
            {"tag_name": "v0.5.1", "draft": false, "prerelease": false, "body": null, "assets": []},
            {"tag_name": "v0.5.2-rc.1", "draft": false, "prerelease": false, "assets": []},
            {"tag_name": "nightly", "draft": false, "prerelease": false, "assets": []},
            {"tag_name": "v0.5.0", "draft": false, "prerelease": false, "assets": []}
        ]"#;
        let r = newest_stable(list).unwrap().unwrap();
        assert_eq!(r.tag, "v0.5.1");
        assert_eq!(newest_stable(b"[]"), Ok(None));
        let only_dev = br#"[{"tag_name": "dev", "prerelease": true}]"#;
        assert_eq!(newest_stable(only_dev), Ok(None));
        assert!(matches!(newest_stable(b"{}"), Err(ReleaseError::Json(_))));
    }

    #[test]
    fn reads_the_dev_build_version_and_date_from_the_body() {
        let json = r#"{"tag_name": "dev", "draft": false, "prerelease": true,
            "body": "Rolling build of the develop branch.\r\n\r\nversion: 0.5.1-dev.23ed277\r\ndate: 2026-11-09\r\n",
            "assets": [{"name": "callgraph-linux-x86_64.zip",
                "browser_download_url": "https://github.com/GeRiY/callgraph/releases/download/dev/callgraph-linux-x86_64.zip"}]}"#;
        let d = DevBuild::from_json(json.as_bytes()).unwrap();
        assert_eq!(d.release.tag, "dev");
        assert_eq!(d.release.version.to_string(), "0.5.1-dev.23ed277");
        assert_eq!(d.date.as_deref(), Some("2026-11-09"));
        assert!(d.release.asset("callgraph-linux-x86_64.zip").is_some());

        let no_date = json.replace("date: 2026-11-09", "");
        assert_eq!(DevBuild::from_json(no_date.as_bytes()).unwrap().date, None);
        for bad in ["version: 0.5.1", "version: soon", ""] {
            let j = json.replace("version: 0.5.1-dev.23ed277", bad);
            assert_eq!(
                DevBuild::from_json(j.as_bytes()),
                Err(ReleaseError::BadTag("dev".into())),
                "{bad}"
            );
        }
        let draft = json.replace(r#""draft": false"#, r#""draft": true"#);
        assert!(matches!(
            DevBuild::from_json(draft.as_bytes()),
            Err(ReleaseError::NotStable(_))
        ));
    }
}
