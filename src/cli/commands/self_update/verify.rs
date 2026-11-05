//! A letöltött zip SHA-256-ellenőrzése telepítés előtt.
//! Kiadásból: a kiadás `SHA256SUMS` assetje kötelező (hiánya / eltérés hiba, semmi nem települ), kivéve
//! `--insecure-skip-checksum`. `--from`: a `--sha256 <hex>`, vagy a zip melletti `SHA256SUMS` szerint; ha egyik
//! sincs, egysoros figyelmeztetés, és a telepítés folytatódik (a helyi fájl a felhasználó döntése).

use std::io::Write;

use super::messages::{
    checksum_mismatch, checksum_missing, checksum_no_entry, fetch_failure, warn_skipped,
    warn_unverified,
};
use super::{Options, SelfUpdateCommand};
use crate::skill::SkillFailure;
use crate::update::checksum::{SUMS_FILE, SUMS_LIMIT, expected_sum, file_name, sibling, verify};
use crate::update::{FetchError, Release};

type Checked = std::result::Result<(), SkillFailure>;

impl SelfUpdateCommand {
    /// Egy kiadás platform-zipjének ellenőrzése a kiadás `SHA256SUMS`-a szerint.
    pub(super) fn verify_release_asset(
        &self,
        o: &Options,
        release: &Release,
        asset_url: &str,
        asset_name: &str,
        zip: &[u8],
        err: &mut dyn Write,
    ) -> Checked {
        if o.skip_checksum {
            let _ = writeln!(err, "{}", warn_skipped(asset_name));
            return Ok(());
        }
        let sums_url = release.asset(SUMS_FILE).map_or_else(
            || sibling(asset_url, SUMS_FILE),
            |a| a.browser_download_url.clone(),
        );
        match self.env.fetcher.get(&sums_url, SUMS_LIMIT) {
            Ok(sums) => check_sums(&sums, &sums_url, asset_name, zip),
            Err(FetchError::NotFound) => Err(checksum_missing(&release.tag, &sums_url)),
            Err(e) => Err(fetch_failure(&sums_url, e)),
        }
    }

    /// Egy `--from` zip ellenőrzése: `--sha256` › a mellette lévő `SHA256SUMS` › figyelmeztetés.
    pub(super) fn verify_local(
        &self,
        o: &Options,
        from: &str,
        zip: &[u8],
        err: &mut dyn Write,
    ) -> Checked {
        let name = file_name(from);
        if o.skip_checksum {
            let _ = writeln!(err, "{}", warn_skipped(name));
            return Ok(());
        }
        if let Some(expected) = &o.sha256 {
            return verify(zip, expected).map_err(|m| checksum_mismatch(name, m));
        }
        let sums_url = sibling(from, SUMS_FILE);
        match self.env.fetcher.get(&sums_url, SUMS_LIMIT) {
            Ok(sums) => check_sums(&sums, &sums_url, name, zip),
            Err(FetchError::NotFound) => {
                let _ = writeln!(err, "{}", warn_unverified(name));
                Ok(())
            }
            Err(e) => Err(fetch_failure(&sums_url, e)),
        }
    }
}

/// A `name` sorának megkeresése a `SHA256SUMS`-ban és a zip összevetése vele.
fn check_sums(sums: &[u8], sums_url: &str, name: &str, zip: &[u8]) -> Checked {
    let sums = String::from_utf8_lossy(sums);
    let expected = expected_sum(&sums, name).ok_or_else(|| checksum_no_entry(name, sums_url))?;
    verify(zip, &expected).map_err(|m| checksum_mismatch(name, m))
}
