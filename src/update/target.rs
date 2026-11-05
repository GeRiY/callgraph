//! Hova telepít a frissítés: a futó bináris skill-mappája (`<skill>/bin/<os>-<arch>/callgraph` vagy `<skill>/bin/callgraph`).
//! Ha a bináris nem egy skill-telepítésben fut (pl. `target/release/callgraph`), nincs cél — a frissítés megtagadja.

use std::path::{Path, PathBuf};

/// A skill-mappát jelző fájlok: legalább az egyiknek ott kell lennie, különben nem skill-telepítés.
const MARKERS: [&str; 2] = ["callgraph.sh", "SKILL.md"];

/// A frissítés célja: a skill-mappa és benne a futó bináris (mindkettő kanonikus út).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InstallTarget {
    skill_dir: PathBuf,
    exe: PathBuf,
}

impl InstallTarget {
    /// A futó bináris (`std::env::current_exe`) célja.
    pub(crate) fn current() -> Option<Self> {
        Self::locate(&std::env::current_exe().ok()?)
    }

    /// A bináris fölötti első `bin` mappa (legfeljebb két szint) szülője, ha az skill-mappa (`callgraph.sh` / `SKILL.md`).
    pub(crate) fn locate(exe: &Path) -> Option<Self> {
        let exe = exe.canonicalize().ok()?;
        let skill_dir = exe
            .ancestors()
            .skip(1)
            .take(2)
            .find(|p| p.file_name().is_some_and(|n| n == "bin"))
            .and_then(Path::parent)?
            .to_path_buf();
        MARKERS
            .iter()
            .any(|m| skill_dir.join(m).is_file())
            .then_some(Self { skill_dir, exe })
    }

    pub(crate) fn skill_dir(&self) -> &Path {
        &self.skill_dir
    }

    pub(crate) fn exe(&self) -> &Path {
        &self.exe
    }
}

/// A Windows-on félretett régi bináris (`callgraph.exe.old`) útja.
pub(crate) fn stale_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_skill_dir_only_for_skill_installs() {
        let root = std::env::temp_dir().join(format!("callgraph-target-{}", std::process::id()));
        let skill = root.join("skill");
        let bin = skill.join("bin/linux-x86_64");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("callgraph"), b"x").unwrap();
        std::fs::write(skill.join("bin/callgraph"), b"x").unwrap();
        // nincs jelzőfájl → nem skill-mappa
        assert_eq!(InstallTarget::locate(&bin.join("callgraph")), None);
        std::fs::write(skill.join("callgraph.sh"), b"#!/bin/sh").unwrap();
        let t = InstallTarget::locate(&bin.join("callgraph")).unwrap();
        assert_eq!(t.skill_dir(), skill.canonicalize().unwrap());
        assert_eq!(t.exe(), bin.join("callgraph").canonicalize().unwrap());
        assert!(InstallTarget::locate(&skill.join("bin/callgraph")).is_some());
        // fejlesztői build: nincs `bin` ős
        let dev = root.join("target/release");
        std::fs::create_dir_all(&dev).unwrap();
        std::fs::write(dev.join("callgraph"), b"x").unwrap();
        assert_eq!(InstallTarget::locate(&dev.join("callgraph")), None);
        assert_eq!(
            stale_path(Path::new("/a/callgraph.exe")),
            PathBuf::from("/a/callgraph.exe.old")
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
