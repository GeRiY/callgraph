//! A `self-update` hibaüzenetei (kétnyelvűen): használat, letöltés, kiadás-forrás, telepítés.

use std::path::Path;

use crate::skill::SkillFailure;
use crate::update::checksum::Mismatch;
use crate::update::{FetchError, InstallError, ReleaseError, SourceError};
use crate::{t, tf};

pub(super) fn usage() -> SkillFailure {
    SkillFailure::usage(t!(
        "usage: callgraph self-update [--check] [--force] [--from <zip|URL> [--sha256 <hex>]] [--insecure-skip-checksum]",
        "használat: callgraph self-update [--check] [--force] [--from <zip|URL> [--sha256 <hex>]] [--insecure-skip-checksum]"
    ))
}

pub(super) fn failure(context: &'static str, message: String) -> SkillFailure {
    SkillFailure { context, message }
}

pub(super) fn fetch_failure(url: &str, e: FetchError) -> SkillFailure {
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

pub(super) fn install_failure(e: InstallError, dir: &Path) -> SkillFailure {
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

/// A kiadás-forrás hibája: letöltési hiba, vagy nem használható kiadás.
pub(super) fn source_failure(url: &str, e: SourceError) -> SkillFailure {
    let e = match e {
        SourceError::Fetch(e) => return fetch_failure(url, e),
        SourceError::Release(e) => e,
    };
    failure(
        "bad_release",
        match e {
            ReleaseError::Json(e) => tf!(
                "unexpected release response from {url}: {e}",
                "váratlan kiadás-válasz innen: {url}: {e}"
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
}

/// A kiadásnak nincs `SHA256SUMS`-a (pl. a v0.1.0) — semmi nem települ.
pub(super) fn checksum_missing(tag: &str, url: &str) -> SkillFailure {
    failure(
        "checksum",
        tf!(
            "release {tag} has no SHA256SUMS ({url}), so the download cannot be verified; nothing was installed. Releases before checksums (v0.1.0) lack it — to install anyway: update --insecure-skip-checksum",
            "a(z) {tag} kiadásnak nincs SHA256SUMS fájlja ({url}), így a letöltés nem ellenőrizhető; semmi nem települt. Az ellenőrzőösszeg előtti kiadásokból (v0.1.0) hiányzik — ha mégis telepítenéd: update --insecure-skip-checksum"
        ),
    )
}

/// A `SHA256SUMS`-ban nincs sor a csomagra.
pub(super) fn checksum_no_entry(name: &str, url: &str) -> SkillFailure {
    failure(
        "checksum",
        tf!(
            "{url} lists no checksum for {name}; nothing was installed",
            "a(z) {url} nem tartalmaz ellenőrzőösszeget ehhez: {name}; semmi nem települt"
        ),
    )
}

/// Eltérő SHA-256 — sérült vagy módosított csomag.
pub(super) fn checksum_mismatch(name: &str, m: Mismatch) -> SkillFailure {
    failure(
        "checksum",
        tf!(
            "SHA-256 mismatch for {name} (expected {}, got {}): the download is corrupt or was tampered with; nothing was installed",
            "a(z) {name} SHA-256-ja eltér (várt: {}, kapott: {}): a letöltés sérült vagy módosított; semmi nem települt",
            m.expected,
            m.actual
        ),
    )
}

/// `--from` ellenőrzés nélkül (nincs `--sha256`, és nincs mellette `SHA256SUMS`).
pub(super) fn warn_unverified(name: &str) -> String {
    tf!(
        "warning: {name} not verified (no SHA256SUMS next to it, no --sha256); installing it as given",
        "figyelem: a(z) {name} nincs ellenőrizve (nincs mellette SHA256SUMS, nincs --sha256); telepítés a megadott fájlból"
    )
}

/// `--insecure-skip-checksum`: az ellenőrzés kihagyva.
pub(super) fn warn_skipped(name: &str) -> String {
    tf!(
        "warning: checksum of {name} not verified (--insecure-skip-checksum)",
        "figyelem: a(z) {name} ellenőrzőösszege nincs ellenőrizve (--insecure-skip-checksum)"
    )
}
