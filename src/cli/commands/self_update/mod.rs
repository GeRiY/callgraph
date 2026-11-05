//! `callgraph self-update [--check] [--force] [--from <zip|URL>]` — frissítés a GitHub Releases-ből.
//! `--check`: csak összeveti a legutóbbi kiadást a saját verzióval; anélkül letölti és a skill-mappába telepíti a
//! platform zipjét (`callgraph-<os>-<arch>.zip`). `--force`: akkor is, ha nem újabb; `--from`: helyi zipből / címről.
//! A legutóbbi kiadás forrása: a `releases/latest` webes átirányítása (tartalék: a GitHub API); felülírható:
//! `CALLGRAPH_UPDATE_API` (API-formátumú tükör, teszt; `file://` is lehet). Rejtett mód: `--check --background`
//! — a `wrap` indítja leválasztva; rövid időkorláttal lekérdez, az eredményt az `update-check.json`-ba írja, és
//! bármi történjék, csendben kilép.

mod messages;

use std::io::Write;
use std::time::Duration;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::{SkillFailure, SkillHome, finish};
use crate::tf;
use crate::update::check::{BACKGROUND_ARGS, now_secs, record_latest};
use crate::update::{
    ASSET_LIMIT, BACKGROUND_TIMEOUT, FOREGROUND_TIMEOUT, Fetcher, HttpFetcher, InstallTarget,
    Installer, Platform, Release, ReleaseSource, Version, default_source,
};
use messages::{failure, fetch_failure, install_failure, source_failure, usage};

const SOURCE: &str = "callgraph self-update";

/// A frissítés külső függőségei (DI): letöltő, kiadás-forrás, telepítési cél, platform, saját verzió.
pub(crate) struct UpdateEnv {
    pub(crate) fetcher: Box<dyn Fetcher>,
    pub(crate) source: Box<dyn ReleaseSource>,
    pub(crate) target: Option<InstallTarget>,
    pub(crate) platform: Platform,
    pub(crate) current: Version,
}

impl UpdateEnv {
    /// A valódi környezet: HTTPS-letöltő a megadott időkorláttal, `CALLGRAPH_UPDATE_API` › GitHub,
    /// a futó bináris skill-mappája.
    pub(crate) fn from_env(timeout: Duration) -> Self {
        let platform = Platform::current();
        Self {
            fetcher: Box::new(HttpFetcher::new(timeout)),
            source: default_source(&platform),
            target: InstallTarget::current(),
            platform,
            current: Version::current(),
        }
    }
}

#[derive(Debug, Default, PartialEq)]
struct Options {
    help: bool,
    check: bool,
    force: bool,
    background: bool,
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
        let background = args.iter().any(|a| a == BACKGROUND_ARGS[2]);
        let timeout = if background {
            BACKGROUND_TIMEOUT
        } else {
            FOREGROUND_TIMEOUT
        };
        Self::with_env(args, home, UpdateEnv::from_env(timeout))
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
                "--background" => o.background = true,
                "--from" => o.from = Some(it.next().ok_or_else(usage)?.clone()),
                x if x.starts_with("--from=") => o.from = Some(x[7..].to_owned()),
                _ => return Err(usage()),
            }
        }
        if (o.check && o.from.is_some()) || (o.background && !o.check) {
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
                    self.env.source.url()
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
        let source = &self.env.source;
        source
            .latest(self.env.fetcher.as_ref())
            .map_err(|e| source_failure(source.url(), e))
    }

    /// A háttér-ellenőrzés (`--check --background`): az eredmény az állapotfájlba, kimenet és hibanapló nélkül.
    fn run_background(&self) {
        let latest = self.latest().map(|r| r.map(|r| r.version)).map_err(|_| ());
        record_latest(&self.home, now_secs(), latest);
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
        if self.options().is_ok_and(|o| o.background) {
            self.run_background();
            return Ok(0);
        }
        let result = self.run_update(out);
        finish(&self.home, SOURCE, result, err)
    }
}

#[cfg(test)]
mod tests;
