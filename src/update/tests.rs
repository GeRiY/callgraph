//! A telepítő tesztjei egy helyben épített release zippel, ideiglenes skill-mappában (hálózat nélkül);
//! a segédfüggvényeket a `self-update` parancs tesztjei is használják.

use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;

use super::{InstallError, InstallTarget, Installer, Platform};

/// A tesztek platformja (a host-tól független): `linux-x86_64`.
pub(crate) fn test_platform() -> Platform {
    Platform::from_consts("linux", "x86_64")
}

/// Zip a megadott (név, tartalom) bejegyzésekből.
pub(crate) fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().unix_permissions(0o644);
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// Egy release zip a valódi elrendezéssel (`callgraph/…`, benne a teszt-platform binárisa).
pub(crate) fn release_zip(binary: &[u8]) -> Vec<u8> {
    make_zip(&[
        ("callgraph/SKILL.md", b"new skill"),
        ("callgraph/callgraph.sh", b"#!/usr/bin/env bash\n# new\n"),
        ("callgraph/help/callgraph.en.txt", b"new help"),
        // felhasználói adat a zipben: soha nem írhatja felül a meglévőt
        ("callgraph/projects.json", b"{\"evil\": \"/\"}"),
        ("callgraph/lang", b"en\n"),
        ("callgraph/bin/linux-x86_64/callgraph", binary),
    ])
}

/// Egy ideiglenes, „régi” skill-telepítés: skill-fájlok, felhasználói adat, egy idegen fájl és a régi bináris.
pub(crate) fn fake_skill(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("callgraph-update-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin/linux-x86_64")).unwrap();
    for (name, text) in [
        ("SKILL.md", "old skill"),
        ("callgraph.sh", "# old"),
        ("projects.json", "{\"app\": \"/srv/app\"}\n"),
        ("lang", "hu\n"),
        ("errors.jsonl", "{\"id\":\"x\"}\n"),
        ("feedback.jsonl", "{\"id\":\"y\"}\n"),
        ("mine.txt", "user file"),
        ("bin/linux-x86_64/callgraph", "OLD"),
    ] {
        std::fs::write(dir.join(name), text).unwrap();
    }
    dir
}

pub(crate) fn target_of(skill: &Path) -> InstallTarget {
    InstallTarget::locate(&skill.join("bin/linux-x86_64/callgraph")).unwrap()
}

pub(crate) fn read(path: PathBuf) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn installs_skill_files_and_binary_but_keeps_user_data() {
    let skill = fake_skill("install");
    let target = target_of(&skill);
    let report = Installer::new(test_platform())
        .install(&release_zip(b"NEW"), &target)
        .unwrap();
    assert_eq!(report.files, 4);
    assert_eq!(
        report.binary,
        target.skill_dir().join("bin/linux-x86_64/callgraph")
    );
    assert_eq!(read(skill.join("SKILL.md")), "new skill");
    assert_eq!(
        read(skill.join("callgraph.sh")),
        "#!/usr/bin/env bash\n# new\n"
    );
    assert_eq!(read(skill.join("help/callgraph.en.txt")), "new help");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "NEW");
    assert_eq!(
        read(skill.join("projects.json")),
        "{\"app\": \"/srv/app\"}\n"
    );
    assert_eq!(read(skill.join("lang")), "hu\n");
    assert_eq!(read(skill.join("errors.jsonl")), "{\"id\":\"x\"}\n");
    assert_eq!(read(skill.join("feedback.jsonl")), "{\"id\":\"y\"}\n");
    assert_eq!(read(skill.join("mine.txt")), "user file");
    // nem marad ideiglenes mappa
    let leftovers: Vec<_> = std::fs::read_dir(&skill)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".update"))
        .collect();
    assert!(leftovers.is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &str| {
            std::fs::metadata(skill.join(p))
                .unwrap()
                .permissions()
                .mode()
        };
        assert_eq!(mode("bin/linux-x86_64/callgraph") & 0o111, 0o111);
        assert_eq!(mode("callgraph.sh") & 0o111, 0o111);
        assert_eq!(mode("SKILL.md") & 0o111, 0);
    }
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn rejects_bad_packages_without_touching_the_skill() {
    let skill = fake_skill("reject");
    let target = target_of(&skill);
    let installer = Installer::new(test_platform());
    let cases: Vec<(Vec<u8>, fn(&InstallError) -> bool)> = vec![
        (b"not a zip".to_vec(), |e| {
            matches!(e, InstallError::NotZip(_))
        }),
        (
            make_zip(&[
                ("callgraph/SKILL.md", b"x"),
                ("callgraph/bin/darwin-arm64/callgraph", b"x"),
            ]),
            |e| matches!(e, InstallError::MissingBinary(p) if p == "callgraph/bin/linux-x86_64/callgraph"),
        ),
        (
            make_zip(&[
                ("other/SKILL.md", b"x"),
                ("callgraph/bin/linux-x86_64/callgraph", b"x"),
            ]),
            |e| matches!(e, InstallError::UnsafeEntry(_)),
        ),
        (
            make_zip(&[
                ("callgraph/../evil", b"x"),
                ("callgraph/bin/linux-x86_64/callgraph", b"x"),
            ]),
            |e| matches!(e, InstallError::UnsafeEntry(_)),
        ),
    ];
    for (zip, expected) in cases {
        let e = installer.install(&zip, &target).unwrap_err();
        assert!(expected(&e), "{e:?}");
    }
    assert_eq!(read(skill.join("SKILL.md")), "old skill");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "OLD");
    assert!(!skill.parent().unwrap().join("evil").exists());
    let _ = std::fs::remove_dir_all(skill);
}
