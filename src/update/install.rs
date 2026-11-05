//! A release zip (`callgraph/…`) telepítése a skill-mappába: a skill-fájlok és a `bin/` felülírása, a felhasználói
//! adatok (`projects.json`, `errors.jsonl`, `feedback.jsonl`, `lang`, `update-check.json`) és a zipben nem szereplő fájlok érintetlenek.
//!
//! Menete: ellenőrzés (zip, biztonságos utak, benne a platform binárisa) → kicsomagolás egy ideiglenes mappába a
//! skill-mappán belül → átnevezés a helyére. A futó bináris cseréje: Unixon átnevezés fölé; Windowson előbb a futót
//! `callgraph.exe.old`-ra nevezi (a következő futás törli).

use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use zip::ZipArchive;

use super::platform::Platform;
use super::target::{InstallTarget, stale_path};

/// A zip gyökérmappája.
const ROOT: &str = "callgraph";

/// Felhasználói adat a skill-mappában — a zip akkor sem írhatja felül, ha (tévedésből) benne van.
const USER_DATA: [&str; 5] = [
    "projects.json",
    "errors.jsonl",
    "feedback.jsonl",
    "lang",
    super::check::STATE_FILE,
];

/// Miért nem telepíthető a csomag.
#[derive(Debug)]
pub(crate) enum InstallError {
    /// Nem zip (vagy sérült).
    NotZip(String),
    /// Hiányzik a platform binárisa (`callgraph/bin/<os>-<arch>/callgraph[.exe]`).
    MissingBinary(String),
    /// A `callgraph/` mappán kívüli vagy kilépő (`..`, abszolút) út.
    UnsafeEntry(String),
    /// Írási hiba.
    Io(std::io::Error),
}

impl From<std::io::Error> for InstallError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// A telepítés eredménye.
#[derive(Debug)]
pub(crate) struct InstallReport {
    /// A felülírt / létrehozott fájlok száma.
    pub(crate) files: usize,
    /// Az új bináris helye.
    pub(crate) binary: PathBuf,
}

/// Egy kicsomagolandó fájl: a zip-index és a skill-mappához viszonyított út.
struct Entry {
    index: usize,
    rel: PathBuf,
    mode: Option<u32>,
}

/// A release zip telepítője egy platformra.
pub(crate) struct Installer {
    platform: Platform,
}

impl Installer {
    pub(crate) fn new(platform: Platform) -> Self {
        Self { platform }
    }

    /// A zip telepítése a cél skill-mappájába (lásd a modul leírását).
    pub(crate) fn install(
        &self,
        zip: &[u8],
        target: &InstallTarget,
    ) -> Result<InstallReport, InstallError> {
        let mut archive =
            ZipArchive::new(Cursor::new(zip)).map_err(|e| InstallError::NotZip(e.to_string()))?;
        let entries = self.entries(&mut archive)?;
        let staging = target
            .skill_dir()
            .join(format!(".update-staging-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&staging);
        let result = self
            .extract(&mut archive, &entries, &staging)
            .and_then(|()| self.move_into_place(&entries, &staging, target));
        let _ = std::fs::remove_dir_all(&staging);
        result?;
        self.finalize(target.skill_dir());
        Ok(InstallReport {
            files: entries.len(),
            binary: target.skill_dir().join(self.binary_rel()),
        })
    }

    /// A bináris útja a skill-mappán belül: `bin/<os>-<arch>/callgraph[.exe]`.
    fn binary_rel(&self) -> PathBuf {
        Path::new("bin")
            .join(self.platform.dir())
            .join(self.platform.binary_name())
    }

    /// A telepítendő fájlok (a mappák és a felhasználói adat nélkül); minden út a `callgraph/` alatt, kilépés nélkül.
    fn entries(&self, archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Vec<Entry>, InstallError> {
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let file = archive
                .by_index(index)
                .map_err(|e| InstallError::NotZip(e.to_string()))?;
            let name = file.name().map(|n| n.into_owned()).unwrap_or_default();
            let rel = file
                .enclosed_name()
                .and_then(|p| p.strip_prefix(ROOT).ok().map(Path::to_path_buf))
                .filter(|p| p.components().all(|c| matches!(c, Component::Normal(_))))
                .ok_or_else(|| InstallError::UnsafeEntry(name.clone()))?;
            let user_data =
                rel.components().count() == 1 && USER_DATA.iter().any(|d| rel.as_os_str() == *d);
            if file.is_dir() || rel.as_os_str().is_empty() || user_data {
                continue;
            }
            let mode = file.unix_mode();
            entries.push(Entry { index, rel, mode });
        }
        let binary = self.binary_rel();
        if !entries.iter().any(|e| e.rel == binary) {
            return Err(InstallError::MissingBinary(self.platform.binary_in_zip()));
        }
        Ok(entries)
    }

    fn extract(
        &self,
        archive: &mut ZipArchive<Cursor<&[u8]>>,
        entries: &[Entry],
        staging: &Path,
    ) -> Result<(), InstallError> {
        for entry in entries {
            let mut file = archive
                .by_index(entry.index)
                .map_err(|e| InstallError::NotZip(e.to_string()))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|e| InstallError::NotZip(e.to_string()))?;
            let path = staging.join(&entry.rel);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, bytes)?;
            set_mode(&path, &entry.rel, entry.mode)?;
        }
        Ok(())
    }

    fn move_into_place(
        &self,
        entries: &[Entry],
        staging: &Path,
        target: &InstallTarget,
    ) -> Result<(), InstallError> {
        for entry in entries {
            let dest = target.skill_dir().join(&entry.rel);
            if let Some(dir) = dest.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let staged = staging.join(&entry.rel);
            if dest == target.exe() {
                replace_running(&staged, &dest)?;
            } else {
                std::fs::rename(&staged, &dest)?;
            }
        }
        Ok(())
    }

    /// Az `install.sh` lépései közül, ami a fájlokon kívül kell: macOS-en a karantén-jelző levétele (ha van `xattr`).
    fn finalize(&self, skill_dir: &Path) {
        if self.platform.is_darwin() {
            let _ = std::process::Command::new("xattr")
                .args(["-dr", "com.apple.quarantine"])
                .arg(skill_dir.join("bin"))
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

/// A futó bináris cseréje: Windowson a futó fájl nem írható felül, csak átnevezhető — előbb `.old`-ra nevezzük.
fn replace_running(staged: &Path, dest: &Path) -> std::io::Result<()> {
    if cfg!(windows) && dest.exists() {
        let old = stale_path(dest);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(dest, &old)?;
    }
    std::fs::rename(staged, dest)
}

/// Unix-jogok: a zipben tárolt mód (ha van), a scriptek és a binárisok futtathatók (mint az `install.sh` `chmod +x`-e).
#[cfg(unix)]
fn set_mode(path: &Path, rel: &Path, mode: Option<u32>) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let executable = rel.extension().is_some_and(|e| e == "sh")
        || ["bin", "extras/hooks", "extras/tools"]
            .iter()
            .any(|dir| rel.parent().is_some_and(|p| p.starts_with(dir)));
    let mut mode = mode.map_or(0o644, |m| m & 0o777) | 0o600;
    if executable {
        mode |= 0o755;
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _rel: &Path, _mode: Option<u32>) -> std::io::Result<()> {
    Ok(())
}
