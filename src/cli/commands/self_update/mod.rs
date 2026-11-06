//! `callgraph self-update [--check] [--force] [--from <zip|URL> [--sha256 <hex>]] [--insecure-skip-checksum]` —
//! frissítés a GitHub Releases-ből. `--check`: csak összeveti a legutóbbi kiadást a saját verzióval (az eredményt a
//! háttér-ellenőrzés állapotfájljába is írja); anélkül letölti, a kiadás `SHA256SUMS`-a szerint ellenőrzi és a
//! skill-mappába telepíti a platform zipjét (`callgraph-<os>-<arch>.zip`). `--force`: akkor is, ha nem újabb;
//! `--from`: helyi zipből / címről (ellenőrzés: `--sha256`, vagy a mellette lévő `SHA256SUMS`; lásd `verify`).
//! A legutóbbi kiadás forrása: a `releases/latest` webes átirányítása (tartalék: a GitHub API); felülírható:
//! `CALLGRAPH_UPDATE_API` (API-formátumú tükör, teszt; `file://` is lehet). Rejtett mód: `--check --background`
//! — a `wrap` indítja leválasztva; rövid időkorláttal lekérdez, az eredményt az `update-check.json`-ba írja, és
//! bármi történjék, csendben kilép.

mod messages;
mod verify;

use std::io::Write;
use std::time::Duration;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::{SkillFailure, SkillHome, finish};
use crate::tf;
use crate::update::check::{BACKGROUND_ARGS, now_secs, record_latest};
use crate::update::checksum::normalize_hex;
use crate::update::{
    ASSET_LIMIT, AUTO_UPDATE_CONNECT_TIMEOUT, AUTO_UPDATE_TIMEOUT, BACKGROUND_TIMEOUT,
    FOREGROUND_TIMEOUT, Fetcher, HttpFetcher, InstallTarget, Installer, Platform, Release,
    ReleaseSource, Version, default_source,
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
        Self::with_fetcher(Box::new(HttpFetcher::new(timeout)))
    }

    /// A valódi környezet a megadott letöltővel.
    pub(crate) fn with_fetcher(fetcher: Box<dyn Fetcher>) -> Self {
        let platform = Platform::current();
        Self {
            fetcher,
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
    /// `--sha256 <hex>`: a `--from` zip várt összege (kisbetűs hexa).
    sha256: Option<String>,
    /// `--insecure-skip-checksum`: nincs SHA-256-ellenőrzés (pl. a `SHA256SUMS` nélküli v0.1.0-hoz).
    skip_checksum: bool,
}

/// A `--sha256` értéke: 64 hexa számjegy, különben használati hiba.
fn sha256_arg(value: Option<&str>) -> std::result::Result<String, SkillFailure> {
    value.and_then(normalize_hex).ok_or_else(usage)
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

    /// Az MCP `--auto-update` lépéséhez: rövid időkorlátok (3 s kapcsolódás, 30 s kérésenként).
    pub(crate) fn for_auto_update(home: SkillHome) -> Self {
        let fetcher = HttpFetcher::with_connect(AUTO_UPDATE_TIMEOUT, AUTO_UPDATE_CONNECT_TIMEOUT);
        Self::with_env(Vec::new(), home, UpdateEnv::with_fetcher(Box::new(fetcher)))
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
                "--sha256" => o.sha256 = Some(sha256_arg(it.next().map(String::as_str))?),
                x if x.starts_with("--sha256=") => o.sha256 = Some(sha256_arg(Some(&x[9..]))?),
                "--insecure-skip-checksum" => o.skip_checksum = true,
                _ => return Err(usage()),
            }
        }
        let bad_sha = o.sha256.is_some() && (o.from.is_none() || o.skip_checksum);
        if (o.check && o.from.is_some()) || (o.background && !o.check) || bad_sha {
            return Err(usage());
        }
        Ok(o)
    }

    fn run_update(
        &self,
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> std::result::Result<(), SkillFailure> {
        let o = self.options()?;
        if o.help {
            writeln!(out, "{}", usage().message)?;
            return Ok(());
        }
        let current = &self.env.current;
        if let Some(from) = &o.from {
            let target = self.target()?;
            let zip = self.download(from)?;
            self.verify_local(&o, from, &zip, err)?;
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
        let release = self.latest()?;
        if self.env.target.is_some() {
            // a kézi lekérdezés is frissíti a háttér-ellenőrzés állapotát (ne ismételje meg; a notice lássa)
            let latest = release.as_ref().map(|r| r.version.clone());
            record_latest(&self.home, now_secs(), Ok(latest));
        }
        let Some(release) = release else {
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
        let summary = self.install_release(&o, &release, err)?;
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

    /// Egy kiadás platform-zipjének letöltése, ellenőrzése (`SHA256SUMS`, lásd `verify`) és telepítése; az
    /// eredmény rövid összefoglalója.
    fn install_release(
        &self,
        o: &Options,
        release: &Release,
        err: &mut dyn Write,
    ) -> std::result::Result<String, SkillFailure> {
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
        self.verify_release_asset(
            o,
            release,
            &asset.browser_download_url,
            &asset_name,
            &zip,
            err,
        )?;
        self.install(&zip, target)
    }

    /// Az MCP-szerver `--auto-update` lépése: ha a legutóbbi stabil kiadás újabb a sajátnál, letölti, a
    /// `SHA256SUMS` szerint ellenőrzi (kötelező — kihagyási lehetőség nincs) és telepíti, kimenet nélkül.
    /// `Ok(Some(..))`: a telepített verzió és az új bináris útja; `Ok(None)`: nincs újabb (vagy nincs kiadás).
    /// Az eredményt (a `latest`-et) az állapotfájlba is írja, mint a `--check`.
    pub(crate) fn update_if_newer(
        &self,
    ) -> std::result::Result<Option<(Version, std::path::PathBuf)>, SkillFailure> {
        let binary = self.target()?.exe().to_path_buf();
        let release = self.latest()?;
        let latest = release.as_ref().map(|r| r.version.clone());
        record_latest(&self.home, now_secs(), Ok(latest));
        let Some(release) = release.filter(|r| r.version > self.env.current) else {
            return Ok(None);
        };
        self.install_release(&Options::default(), &release, &mut std::io::sink())?;
        Ok(Some((release.version, binary)))
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
        crate::skill::sync_installed_skill(target.skill_dir()); // a SKILL.md a felhasználó nyelvén (a zipben angol)
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
        let result = self.run_update(out, err);
        finish(&self.home, SOURCE, result, err)
    }
}

#[cfg(test)]
mod tests;
