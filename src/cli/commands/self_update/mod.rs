//! `callgraph self-update [--check] [--force] [--from <zip|URL>]` — frissítés a GitHub Releases-ből.
//! `--check`: csak összeveti a legutóbbi kiadást a saját verzióval; anélkül letölti és a skill-mappába telepíti a
//! platform zipjét (`callgraph-<os>-<arch>.zip`). `--force`: akkor is, ha nem újabb; `--from`: helyi zipből / címről.
//! A kiadás-lekérdezés címe felülírható: `CALLGRAPH_UPDATE_API` (tükör, teszt; `file://` is lehet).

use std::io::Write;
use std::path::Path;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::{SkillFailure, SkillHome, finish};
use crate::update::{
    API_LIMIT, ASSET_LIMIT, DEFAULT_API, FetchError, Fetcher, HttpFetcher, InstallError,
    InstallTarget, Installer, Platform, Release, ReleaseError, Version,
};
use crate::{t, tf};

const SOURCE: &str = "callgraph self-update";

/// A frissítés külső függőségei (DI): letöltő, API-cím, telepítési cél, platform, saját verzió.
pub(crate) struct UpdateEnv {
    pub(crate) fetcher: Box<dyn Fetcher>,
    pub(crate) api: String,
    pub(crate) target: Option<InstallTarget>,
    pub(crate) platform: Platform,
    pub(crate) current: Version,
}

impl UpdateEnv {
    /// A valódi környezet: HTTPS-letöltő, `CALLGRAPH_UPDATE_API` › GitHub, a futó bináris skill-mappája.
    pub(crate) fn from_env() -> Self {
        Self {
            fetcher: Box::new(HttpFetcher::new()),
            api: std::env::var("CALLGRAPH_UPDATE_API")
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| DEFAULT_API.to_owned()),
            target: InstallTarget::current(),
            platform: Platform::current(),
            current: Version::current(),
        }
    }
}

#[derive(Debug, Default, PartialEq)]
struct Options {
    help: bool,
    check: bool,
    force: bool,
    from: Option<String>,
}

/// A `self-update` parancs.
pub struct SelfUpdateCommand {
    args: Vec<String>,
    home: SkillHome,
    env: UpdateEnv,
}

impl SelfUpdateCommand {
    pub fn new(args: Vec<String>, home: SkillHome) -> Self {
        Self::with_env(args, home, UpdateEnv::from_env())
    }

    pub(crate) fn with_env(args: Vec<String>, home: SkillHome, env: UpdateEnv) -> Self {
        Self { args, home, env }
    }

    fn options(&self) -> std::result::Result<Options, SkillFailure> {
        let mut o = Options::default();
        let mut it = self.args.iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--help" | "-h" => o.help = true,
                "--check" => o.check = true,
                "--force" => o.force = true,
                "--from" => o.from = Some(it.next().ok_or_else(usage)?.clone()),
                x if x.starts_with("--from=") => o.from = Some(x[7..].to_owned()),
                _ => return Err(usage()),
            }
        }
        if o.check && o.from.is_some() {
            return Err(usage());
        }
        Ok(o)
    }

    fn run_update(&self, out: &mut dyn Write) -> std::result::Result<(), SkillFailure> {
        let o = self.options()?;
        if o.help {
            writeln!(out, "{}", usage().message)?;
            return Ok(());
        }
        let current = &self.env.current;
        if let Some(from) = &o.from {
            let target = self.target()?;
            let zip = self.download(from)?;
            let summary = self.install(&zip, target)?;
            writeln!(
                out,
                "{}",
                tf!(
                    "Installed from {from} (was {current}; {summary})",
                    "Telepítve innen: {from} (előtte {current}; {summary})"
                )
            )?;
            return Ok(());
        }
        let Some(release) = self.latest()? else {
            writeln!(
                out,
                "{}",
                tf!(
                    "No release published yet ({}); current version: {current}",
                    "Még nincs kiadás ({}); a jelenlegi verzió: {current}",
                    self.env.api
                )
            )?;
            return Ok(());
        };
        let latest = &release.version;
        if latest <= current && (!o.force || o.check) {
            writeln!(
                out,
                "{}",
                tf!(
                    "callgraph is up to date ({current})",
                    "A callgraph naprakész ({current})"
                )
            )?;
            return Ok(());
        }
        if o.check {
            writeln!(
                out,
                "{}",
                tf!(
                    "Update available: {current} → {latest}",
                    "Frissítés elérhető: {current} → {latest}"
                )
            )?;
            let cmd = self.home.script("callgraph.sh");
            writeln!(out, "{}", tf!("Run: {cmd} update", "Futtasd: {cmd} update"))?;
            return Ok(());
        }
        let target = self.target()?;
        let asset_name = self.env.platform.asset_name();
        let Some(asset) = release.asset(&asset_name) else {
            return Err(failure(
                "not_found",
                tf!(
                    "release {} has no package for this platform ({asset_name})",
                    "a(z) {} kiadásban nincs csomag ehhez a platformhoz ({asset_name})",
                    release.tag
                ),
            ));
        };
        let zip = self.download(&asset.browser_download_url)?;
        let summary = self.install(&zip, target)?;
        let line = if latest == current {
            tf!("Reinstalled: {latest}", "Újratelepítve: {latest}")
        } else {
            tf!(
                "Updated: {current} → {latest}",
                "Frissítve: {current} → {latest}"
            )
        };
        writeln!(out, "{line} ({summary})")?;
        Ok(())
    }

    /// A legutóbbi stabil kiadás; `None`, ha még nincs (404).
    fn latest(&self) -> std::result::Result<Option<Release>, SkillFailure> {
        let body = match self.env.fetcher.get(&self.env.api, API_LIMIT) {
            Ok(body) => body,
            Err(FetchError::NotFound) => return Ok(None),
            Err(e) => return Err(fetch_failure(&self.env.api, e)),
        };
        Release::from_json(&body).map(Some).map_err(|e| {
            let api = &self.env.api;
            failure(
                "bad_release",
                match e {
                    ReleaseError::Json(e) => tf!(
                        "unexpected release response from {api}: {e}",
                        "váratlan kiadás-válasz innen: {api}: {e}"
                    ),
                    ReleaseError::BadTag(tag) => tf!(
                        "the latest release tag is not a version: {tag}",
                        "a legutóbbi kiadás tagje nem verzió: {tag}"
                    ),
                    ReleaseError::NotStable(tag) => tf!(
                        "the latest release is a draft or pre-release: {tag}",
                        "a legutóbbi kiadás vázlat vagy előzetes: {tag}"
                    ),
                },
            )
        })
    }

    fn download(&self, url: &str) -> std::result::Result<Vec<u8>, SkillFailure> {
        self.env
            .fetcher
            .get(url, ASSET_LIMIT)
            .map_err(|e| fetch_failure(url, e))
    }

    fn target(&self) -> std::result::Result<&InstallTarget, SkillFailure> {
        self.env.target.as_ref().ok_or_else(|| {
            let exe = std::env::current_exe().unwrap_or_default();
            let dir = self.env.platform.dir();
            failure(
                "not_skill",
                tf!(
                    "self-update only updates a skill installation (<skill>/bin/{dir}/callgraph next to callgraph.sh); this binary is not one: {}\nUpdate the installed skill instead (bash ~/.claude/skills/callgraph/callgraph.sh update), or rebuild from source.",
                    "a self-update csak skill-telepítést frissít (<skill>/bin/{dir}/callgraph, mellette callgraph.sh); ez a bináris nem az: {}\nA telepített skillt frissítsd (bash ~/.claude/skills/callgraph/callgraph.sh update), vagy építsd újra forrásból.",
                    exe.display()
                ),
            )
        })
    }

    /// A telepítés; az eredmény rövid összefoglalója: `<n> files, binary: <út>`.
    fn install(
        &self,
        zip: &[u8],
        target: &InstallTarget,
    ) -> std::result::Result<String, SkillFailure> {
        let report = Installer::new(self.env.platform.clone())
            .install(zip, target)
            .map_err(|e| install_failure(e, target.skill_dir()))?;
        Ok(tf!(
            "{} files, binary: {}",
            "{} fájl, bináris: {}",
            report.files,
            report.binary.display()
        ))
    }
}

impl Command for SelfUpdateCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let result = self.run_update(out);
        finish(&self.home, SOURCE, result, err)
    }
}

fn usage() -> SkillFailure {
    SkillFailure::usage(t!(
        "usage: callgraph self-update [--check] [--force] [--from <zip|URL>]",
        "használat: callgraph self-update [--check] [--force] [--from <zip|URL>]"
    ))
}

fn failure(context: &'static str, message: String) -> SkillFailure {
    SkillFailure { context, message }
}

fn fetch_failure(url: &str, e: FetchError) -> SkillFailure {
    let message = match e {
        FetchError::NotFound => tf!("not found: {url}", "nem található: {url}"),
        FetchError::Status(code) => tf!(
            "download failed (HTTP {code}): {url}",
            "a letöltés sikertelen (HTTP {code}): {url}"
        ),
        FetchError::Other(e) => tf!(
            "download failed: {url}: {e}",
            "a letöltés sikertelen: {url}: {e}"
        ),
    };
    failure("network", message)
}

fn install_failure(e: InstallError, dir: &Path) -> SkillFailure {
    let message = match e {
        InstallError::NotZip(e) => tf!(
            "the package is not a valid zip: {e}",
            "a csomag nem érvényes zip: {e}"
        ),
        InstallError::MissingBinary(path) => tf!(
            "the package has no binary for this platform: {path}",
            "a csomagban nincs bináris ehhez a platformhoz: {path}"
        ),
        InstallError::UnsafeEntry(name) => tf!(
            "the package has an entry outside callgraph/: {name}",
            "a csomagban callgraph/ mappán kívüli bejegyzés van: {name}"
        ),
        InstallError::Io(e) => tf!(
            "installing into {} failed: {e}",
            "a telepítés sikertelen ide: {}: {e}",
            dir.display()
        ),
    };
    failure("bad_package", message)
}

#[cfg(test)]
mod tests;
