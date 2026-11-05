//! Beépített frissítés a GitHub Releases-ből (`callgraph self-update [--check] [--force] [--from <zip>]`):
//! a legutóbbi kiadás lekérdezése és összevetése a saját verzióval, a platform zipjének letöltése és telepítése
//! a skill-mappába. A parancs: `cli::commands::SelfUpdateCommand`.

mod fetch;
mod install;
mod platform;
mod release;
mod target;
mod version;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) use fetch::{API_LIMIT, ASSET_LIMIT, FetchError, Fetcher, HttpFetcher};
pub(crate) use install::{InstallError, Installer};
pub(crate) use platform::Platform;
pub(crate) use release::{DEFAULT_API, Release, ReleaseError};
pub(crate) use target::InstallTarget;
pub(crate) use version::Version;

/// A Windows-on egy korábbi frissítésből megmaradt `callgraph.exe.old` törlése (a futó bináris mellől; best effort).
/// Más rendszeren nincs ilyen fájl, ott nem csinál semmit.
pub fn cleanup_stale_binary() {
    if cfg!(windows)
        && let Ok(exe) = std::env::current_exe()
    {
        let _ = std::fs::remove_file(target::stale_path(&exe));
    }
}
