//! A `self-update` hibaüzenetei (kétnyelvűen): használat, letöltés, kiadás-forrás, telepítés.

use std::path::Path;

use crate::skill::SkillFailure;
use crate::update::{FetchError, InstallError, ReleaseError, SourceError};
use crate::{t, tf};

pub(super) fn usage() -> SkillFailure {
    SkillFailure::usage(t!(
        "usage: callgraph self-update [--check] [--force] [--from <zip|URL>]",
        "használat: callgraph self-update [--check] [--force] [--from <zip|URL>]"
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
