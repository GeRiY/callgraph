//! A release platform-neve (`<os>-<arch>`): ugyanaz, mint a skill `bin/` almappái és a release zipek
//! (`skill/lib.sh` `callgraph_bin()`, `.github/workflows/release.yml`).

/// Egy release-platform: `darwin-arm64`, `darwin-x86_64`, `linux-x86_64`, `linux-aarch64`, `windows-x86_64`, ….
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Platform {
    os: String,
    arch: String,
}

impl Platform {
    /// Ennek a binárisnak a platformja (`std::env::consts`).
    pub(crate) fn current() -> Self {
        Self::from_consts(std::env::consts::OS, std::env::consts::ARCH)
    }

    /// A Rust `OS` / `ARCH` nevei a release `uname`-stílusú neveire: `macos` → `darwin`,
    /// `aarch64` → `arm64` csak darwinon (Linuxon `aarch64` marad).
    pub(crate) fn from_consts(os: &str, arch: &str) -> Self {
        let os = match os {
            "macos" => "darwin",
            other => other,
        };
        let arch = match (os, arch) {
            ("darwin", "aarch64") => "arm64",
            (_, other) => other,
        };
        Self {
            os: os.to_owned(),
            arch: arch.to_owned(),
        }
    }

    /// `<os>-<arch>` — a `bin/` almappa.
    pub(crate) fn dir(&self) -> String {
        format!("{}-{}", self.os, self.arch)
    }

    /// Ennek a platformnak a release-assetje: `callgraph-<os>-<arch>.zip`.
    pub(crate) fn asset_name(&self) -> String {
        format!("callgraph-{}.zip", self.dir())
    }

    /// A bináris fájlneve: Windowson `callgraph.exe`, máshol `callgraph`.
    pub(crate) fn binary_name(&self) -> &'static str {
        if self.os == "windows" {
            "callgraph.exe"
        } else {
            "callgraph"
        }
    }

    /// A bináris útja a release zipben: `callgraph/bin/<os>-<arch>/callgraph[.exe]`.
    pub(crate) fn binary_in_zip(&self) -> String {
        format!("callgraph/bin/{}/{}", self.dir(), self.binary_name())
    }

    pub(crate) fn is_darwin(&self) -> bool {
        self.os == "darwin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_rust_consts_to_release_names() {
        let cases = [
            ("macos", "aarch64", "darwin-arm64"),
            ("macos", "x86_64", "darwin-x86_64"),
            ("linux", "x86_64", "linux-x86_64"),
            ("linux", "aarch64", "linux-aarch64"),
            ("windows", "x86_64", "windows-x86_64"),
        ];
        for (os, arch, dir) in cases {
            let p = Platform::from_consts(os, arch);
            assert_eq!(p.dir(), dir);
            assert_eq!(p.asset_name(), format!("callgraph-{dir}.zip"));
        }
        assert_eq!(
            Platform::from_consts("windows", "x86_64").binary_in_zip(),
            "callgraph/bin/windows-x86_64/callgraph.exe"
        );
        assert_eq!(
            Platform::from_consts("linux", "aarch64").binary_in_zip(),
            "callgraph/bin/linux-aarch64/callgraph"
        );
    }
}
