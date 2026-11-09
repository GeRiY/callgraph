//! Beépített frissítés a GitHub Releases-ből (`callgraph self-update [--check] [--force] [--from <zip>]`):
//! a legutóbbi kiadás lekérdezése és összevetése a saját verzióval, a platform zipjének letöltése és telepítése
//! a skill-mappába. A parancs: `cli::commands::SelfUpdateCommand`. A `wrap` utáni automatikus, háttérben futó
//! ellenőrzés: `check`.

pub(crate) mod check;
pub(crate) mod checksum;
mod fetch;
mod install;
mod platform;
mod release;
mod source;
mod target;
mod version;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) use fetch::{
    API_LIMIT, ASSET_LIMIT, AUTO_UPDATE_CONNECT_TIMEOUT, AUTO_UPDATE_TIMEOUT, BACKGROUND_TIMEOUT,
    FOREGROUND_TIMEOUT, FetchError, Fetcher, HttpFetcher,
};
pub(crate) use install::{InstallError, Installer};
pub(crate) use platform::Platform;
pub(crate) use release::{
    DEFAULT_DEV_API, DEFAULT_LIST, DevBuild, Release, ReleaseError, newest_stable,
};
#[cfg(test)]
pub(crate) use source::{ApiSource, RedirectSource};
pub(crate) use source::{ReleaseSource, SourceError, default_source};
pub(crate) use target::InstallTarget;
pub(crate) use version::{Version, build_version};

/// A Windows-on egy korábbi frissítésből megmaradt `callgraph.exe.old` törlése (a futó bináris mellől; best effort).
/// Más rendszeren nincs ilyen fájl, ott nem csinál semmit.
pub fn cleanup_stale_binary() {
    if cfg!(windows)
        && let Ok(exe) = std::env::current_exe()
    {
        let _ = std::fs::remove_file(target::stale_path(&exe));
    }
}
