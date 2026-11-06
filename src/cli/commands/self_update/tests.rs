//! A `self-update` parancs tesztjei hamis letöltővel (DI) — hálózat nélkül.

use std::collections::HashMap;
use std::path::Path;

use super::*;
use crate::update::check::CheckState;
use crate::update::tests::{fake_skill, read, release_zip, target_of, test_platform};
use crate::update::{ApiSource, FetchError, RedirectSource};

const API: &str = "https://example.invalid/releases/latest";
const ASSET: &str = "https://example.invalid/download/callgraph-linux-x86_64.zip";
const WEB: &str = "https://example.invalid/o/callgraph/releases/latest";
const SUMS: &str = "https://example.invalid/download/SHA256SUMS";
const NAME: &str = "callgraph-linux-x86_64.zip";

/// Egy `SHA256SUMS` a megadott zipre (és egy másik sorra), `sha256sum` formátumban.
fn sums(zip: &[u8], name: &str) -> Vec<u8> {
    let other = crate::update::checksum::sha256_hex(b"other");
    let own = crate::update::checksum::sha256_hex(zip);
    format!("{other}  callgraph-0.0.0.tar.gz\n{own}  {name}\n").into_bytes()
}

/// URL → válasz; ami nincs benne, az 404. Az átirányítás célja a `redirect:<url>` kulcs alatti szöveg.
struct FakeFetcher(HashMap<String, std::result::Result<Vec<u8>, FetchError>>);

impl Fetcher for FakeFetcher {
    fn get(&self, url: &str, _limit: u64) -> std::result::Result<Vec<u8>, FetchError> {
        self.0
            .get(url)
            .cloned()
            .unwrap_or(Err(FetchError::NotFound))
    }

    fn redirect(&self, url: &str) -> std::result::Result<String, FetchError> {
        self.get(&format!("redirect:{url}"), 0)
            .map(|b| String::from_utf8(b).unwrap())
    }
}

fn release_json(tag: &str) -> Vec<u8> {
    format!(
        r#"{{"tag_name": "{tag}", "draft": false, "prerelease": false, "assets": [
            {{"name": "callgraph-0.0.0.tar.gz", "browser_download_url": "https://example.invalid/x.tar.gz"}},
            {{"name": "callgraph-linux-x86_64.zip", "browser_download_url": "{ASSET}"}}]}}"#
    )
    .into_bytes()
}

fn home(skill: &Path) -> SkillHome {
    SkillHome::new(
        skill.to_path_buf(),
        skill.display().to_string(),
        skill.join("projects.json"),
    )
}

type Responses<'a> = Vec<(&'a str, std::result::Result<Vec<u8>, FetchError>)>;

/// A parancs futtatása az API-forrással: (kilépési kód, stdout, stderr).
fn run(args: &[&str], skill: &Path, responses: Responses, in_skill: bool) -> (i32, String, String) {
    run_with(
        args,
        skill,
        responses,
        in_skill,
        Box::new(ApiSource::new(API)),
    )
}

/// A parancs futtatása a megadott kiadás-forrással.
fn run_with(
    args: &[&str],
    skill: &Path,
    responses: Responses,
    in_skill: bool,
    source: Box<dyn ReleaseSource>,
) -> (i32, String, String) {
    let env = UpdateEnv {
        fetcher: Box::new(FakeFetcher(
            responses
                .into_iter()
                .map(|(u, r)| (u.to_owned(), r))
                .collect(),
        )),
        source,
        target: in_skill.then(|| target_of(skill)),
        platform: test_platform(),
        current: Version::parse("0.1.0").unwrap(),
    };
    let args = args.iter().map(|a| a.to_string()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = SelfUpdateCommand::with_env(args, home(skill), env)
        .run(&mut out, &mut err)
        .unwrap();
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn check_reports_available_update_or_up_to_date() {
    let skill = fake_skill("cmd-check");
    let (code, out, _) = run(
        &["--check"],
        &skill,
        vec![(API, Ok(release_json("v0.2.0")))],
        true,
    );
    assert_eq!(code, 0);
    assert!(out.contains("0.1.0 → 0.2.0"), "{out}");
    assert!(out.contains("callgraph.sh update"), "{out}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "OLD");

    for tag in ["v0.1.0", "v0.0.9"] {
        let (code, out, _) = run(
            &["--check", "--force"],
            &skill,
            vec![(API, Ok(release_json(tag)))],
            true,
        );
        assert_eq!(code, 0);
        assert!(out.contains("(0.1.0)") && !out.contains('→'), "{out}");
    }
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn missing_release_is_not_an_error_but_network_failure_is() {
    let skill = fake_skill("cmd-net");
    let (code, out, _) = run(&["--check"], &skill, vec![], true);
    assert_eq!(code, 0);
    assert!(out.contains(API) && out.contains("0.1.0"), "{out}");

    let (code, _, err) = run(&[], &skill, vec![(API, Err(FetchError::Status(403)))], true);
    assert_eq!(code, 1);
    assert!(err.contains("403") && err.contains("[error-id: "), "{err}");
    assert!(read(skill.join("errors.jsonl")).lines().count() >= 2);

    let (code, _, err) = run(&["--bogus"], &skill, vec![], true);
    assert_eq!(code, 1);
    assert!(err.contains("self-update [--check]"), "{err}");
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn update_installs_newer_release_and_keeps_user_data() {
    let skill = fake_skill("cmd-update");
    let responses = || {
        vec![
            (API, Ok(release_json("v0.2.0"))),
            (ASSET, Ok(release_zip(b"NEW"))),
            (SUMS, Ok(sums(&release_zip(b"NEW"), NAME))),
        ]
    };
    let (code, out, err) = run(&[], &skill, responses(), true);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("0.1.0 → 0.2.0"), "{out}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "NEW");
    assert_eq!(
        read(skill.join("projects.json")),
        "{\"app\": \"/srv/app\"}\n"
    );
    assert_eq!(read(skill.join("lang")), "hu\n");

    // egyenlő verzió: --force nélkül nem telepít, --force-szal újratelepít
    std::fs::write(skill.join("SKILL.md"), "edited").unwrap();
    let same = || {
        vec![
            (API, Ok(release_json("v0.1.0"))),
            (ASSET, Ok(release_zip(b"SAME"))),
            (SUMS, Ok(sums(&release_zip(b"SAME"), NAME))),
        ]
    };
    let (code, _, _) = run(&[], &skill, same(), true);
    assert_eq!(code, 0);
    assert_eq!(read(skill.join("SKILL.md")), "edited");
    let (code, out, _) = run(&["--force"], &skill, same(), true);
    assert_eq!(code, 0);
    assert!(out.contains("0.1.0") && !out.contains('→'), "{out}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "SAME");
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn installs_from_local_zip_and_refuses_outside_a_skill_dir() {
    let skill = fake_skill("cmd-from");
    let zip = "/tmp/callgraph-test-release.zip";
    let (code, out, err) = run(
        &["--from", zip],
        &skill,
        vec![(zip, Ok(release_zip(b"LOCAL")))],
        true,
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains(zip), "{out}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "LOCAL");

    let responses = vec![
        (API, Ok(release_json("v0.2.0"))),
        (ASSET, Ok(release_zip(b"NEW"))),
    ];
    let (code, _, err) = run(&[], &skill, responses, false);
    assert_eq!(code, 1);
    assert!(
        err.contains("self-update") && err.contains("[error-id: "),
        "{err}"
    );
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "LOCAL");

    let (code, _, _) = run(&["--check", "--from", zip], &skill, vec![], true);
    assert_eq!(code, 1);
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn redirect_source_finds_and_installs_the_latest_release() {
    let skill = fake_skill("cmd-redirect");
    let tag = "https://example.invalid/o/callgraph/releases/tag/v0.2.0";
    let zip =
        "https://example.invalid/o/callgraph/releases/download/v0.2.0/callgraph-linux-x86_64.zip";
    let sums_url = "https://example.invalid/o/callgraph/releases/download/v0.2.0/SHA256SUMS";
    let redirect_key = format!("redirect:{WEB}");
    let source = || Box::new(RedirectSource::new(WEB, &test_platform()));
    let responses = || {
        vec![
            (redirect_key.as_str(), Ok(tag.as_bytes().to_vec())),
            (zip, Ok(release_zip(b"WEB"))),
            (sums_url, Ok(sums(&release_zip(b"WEB"), NAME))),
        ]
    };
    let (code, out, _) = run_with(&["--check"], &skill, responses(), true, source());
    assert_eq!(code, 0);
    assert!(out.contains("0.1.0 → 0.2.0"), "{out}");
    let (code, out, err) = run_with(&[], &skill, responses(), true, source());
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("0.1.0 → 0.2.0"), "{out}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "WEB");

    // nincs kiadás (404): ugyanaz a barátságos üzenet, mint eddig
    let (code, out, _) = run_with(&["--check"], &skill, vec![], true, source());
    assert_eq!(code, 0);
    assert!(out.contains(WEB) && out.contains("0.1.0"), "{out}");
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn background_check_records_latest_silently() {
    let skill = fake_skill("cmd-background");
    let args = ["--check", "--background"];
    let (code, out, err) = run(&args, &skill, vec![(API, Ok(release_json("v0.2.0")))], true);
    assert_eq!((code, out.as_str(), err.as_str()), (0, "", ""));
    let state = CheckState::load(&skill.join("update-check.json"));
    assert_eq!(state.latest.as_deref(), Some("0.2.0"));
    assert!(state.checked_at > 0);

    // hálózati hiba: csendes, a latest marad, nincs hibanapló-sor
    let errors = read(skill.join("errors.jsonl"));
    let (code, out, err) = run(
        &args,
        &skill,
        vec![(API, Err(FetchError::Status(403)))],
        true,
    );
    assert_eq!((code, out.as_str(), err.as_str()), (0, "", ""));
    let state = CheckState::load(&skill.join("update-check.json"));
    assert_eq!(state.latest.as_deref(), Some("0.2.0"));
    assert_eq!(read(skill.join("errors.jsonl")), errors);

    // --background csak --check-kel
    let (code, _, _) = run(&["--background"], &skill, vec![], true);
    assert_eq!(code, 1);
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn release_without_sha256sums_is_refused_unless_skipped() {
    let skill = fake_skill("cmd-nosums");
    let responses = || {
        vec![
            (API, Ok(release_json("v0.2.0"))),
            (ASSET, Ok(release_zip(b"NEW"))),
        ]
    };
    let (code, _, err) = run(&[], &skill, responses(), true);
    assert_eq!(code, 1);
    assert!(
        err.contains("SHA256SUMS") && err.contains("--insecure-skip-checksum"),
        "{err}"
    );
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "OLD");

    let (code, out, err) = run(&["--insecure-skip-checksum"], &skill, responses(), true);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("0.1.0 → 0.2.0"), "{out}");
    assert!(err.contains("--insecure-skip-checksum"), "{err}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "NEW");
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn checksum_mismatch_or_missing_entry_installs_nothing() {
    let skill = fake_skill("cmd-badsum");
    for bad in [
        sums(&release_zip(b"TAMPERED"), NAME),
        sums(&release_zip(b"NEW"), "callgraph-darwin-arm64.zip"),
    ] {
        let responses = vec![
            (API, Ok(release_json("v0.2.0"))),
            (ASSET, Ok(release_zip(b"NEW"))),
            (SUMS, Ok(bad)),
        ];
        let (code, _, err) = run(&[], &skill, responses, true);
        assert_eq!(code, 1);
        assert!(err.contains(NAME) && err.contains("[error-id: "), "{err}");
        assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "OLD");
    }
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn sha256sums_asset_listed_by_the_api_is_used() {
    let skill = fake_skill("cmd-sumasset");
    let listed = "https://example.invalid/elsewhere/SHA256SUMS";
    let json = String::from_utf8(release_json("v0.2.0")).unwrap().replace(
        "]}",
        &format!(r#", {{"name": "SHA256SUMS", "browser_download_url": "{listed}"}}]}}"#),
    );
    let responses = vec![
        (API, Ok(json.into_bytes())),
        (ASSET, Ok(release_zip(b"NEW"))),
        (listed, Ok(sums(&release_zip(b"NEW"), NAME))),
    ];
    let (code, _, err) = run(&[], &skill, responses, true);
    assert_eq!(code, 0, "{err}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "NEW");
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn local_zip_is_verified_by_sha256_or_sibling_sums_else_warned() {
    let skill = fake_skill("cmd-fromsum");
    let zip = "/tmp/dl/callgraph-linux-x86_64.zip";
    let good = crate::update::checksum::sha256_hex(&release_zip(b"LOCAL"));
    let bad = crate::update::checksum::sha256_hex(b"x");

    // nincs mellette SHA256SUMS: figyelmeztetés, telepít
    let only_zip = || vec![(zip, Ok(release_zip(b"LOCAL")))];
    let (code, _, err) = run(&["--from", zip], &skill, only_zip(), true);
    assert_eq!(code, 0, "{err}");
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("SHA256SUMS"), "{err}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "LOCAL");

    // --sha256: egyezés telepít, eltérés nem
    let (code, _, err) = run(
        &["--from", zip, "--sha256", &good],
        &skill,
        only_zip(),
        true,
    );
    assert_eq!((code, err.as_str()), (0, ""));
    std::fs::write(skill.join("bin/linux-x86_64/callgraph"), "OLD").unwrap();
    let arg = format!("--sha256={bad}");
    let (code, _, err) = run(&["--from", zip, &arg], &skill, only_zip(), true);
    assert_eq!(code, 1);
    assert!(err.contains(&bad) && err.contains(&good), "{err}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "OLD");

    // a mellette lévő SHA256SUMS szerint
    let with_sums = |zip_bytes: &[u8]| {
        vec![
            (zip, Ok(release_zip(b"LOCAL"))),
            ("/tmp/dl/SHA256SUMS", Ok(sums(zip_bytes, NAME))),
        ]
    };
    let (code, _, err) = run(&["--from", zip], &skill, with_sums(b"other"), true);
    assert_eq!(code, 1, "{err}");
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "OLD");
    let (code, _, err) = run(
        &["--from", zip],
        &skill,
        with_sums(&release_zip(b"LOCAL")),
        true,
    );
    assert_eq!((code, err.as_str()), (0, ""));
    assert_eq!(read(skill.join("bin/linux-x86_64/callgraph")), "LOCAL");

    // --sha256 csak --from-mal, érvényes hexával
    for args in [
        vec!["--sha256", good.as_str()],
        vec!["--from", zip, "--sha256", "abc"],
        vec!["--from", zip, "--sha256"],
    ] {
        let (code, _, err) = run(&args, &skill, only_zip(), true);
        assert_eq!(code, 1);
        assert!(err.contains("--sha256"), "{err}");
    }
    let _ = std::fs::remove_dir_all(skill);
}

#[test]
fn manual_check_refreshes_the_background_state() {
    let skill = fake_skill("cmd-check-state");
    let file = skill.join("update-check.json");
    let old = CheckState {
        checked_at: 5,
        latest: Some("0.1.0".into()),
        notified_at: Some(7),
        notified_version: Some("0.1.0".into()),
        auto_update_at: None,
    };
    old.save(&file).unwrap();
    let (code, _, _) = run(
        &["--check"],
        &skill,
        vec![(API, Ok(release_json("v0.2.0")))],
        true,
    );
    assert_eq!(code, 0);
    let state = CheckState::load(&file);
    assert_eq!(state.latest.as_deref(), Some("0.2.0"));
    assert!(state.checked_at > 5);
    assert_eq!(
        (state.notified_at, state.notified_version.as_deref()),
        (Some(7), Some("0.1.0"))
    );

    // nincs kiadás: latest = null; hálózati hiba: nem ír
    let (code, _, _) = run(&["--check"], &skill, vec![], true);
    assert_eq!(code, 0);
    assert_eq!(CheckState::load(&file).latest, None);
    old.save(&file).unwrap();
    let (code, _, _) = run(
        &["--check"],
        &skill,
        vec![(API, Err(FetchError::Status(500)))],
        true,
    );
    assert_eq!(code, 1);
    assert_eq!(CheckState::load(&file), old);

    // skill-telepítésen kívül nem ír állapotot
    std::fs::remove_file(&file).unwrap();
    let (code, _, _) = run(
        &["--check"],
        &skill,
        vec![(API, Ok(release_json("v0.2.0")))],
        false,
    );
    assert_eq!(code, 0);
    assert!(!file.exists());
    let _ = std::fs::remove_dir_all(skill);
}

/// Az MCP `--auto-update` lépéséhez: a parancs az API-forrással és a hamis letöltővel, argumentumok nélkül.
fn auto_command(skill: &Path, responses: Responses) -> SelfUpdateCommand {
    let env = UpdateEnv {
        fetcher: Box::new(FakeFetcher(
            responses
                .into_iter()
                .map(|(u, r)| (u.to_owned(), r))
                .collect(),
        )),
        source: Box::new(ApiSource::new(API)),
        target: Some(target_of(skill)),
        platform: test_platform(),
        current: Version::parse("0.1.0").unwrap(),
    };
    SelfUpdateCommand::with_env(Vec::new(), home(skill), env)
}

#[test]
fn update_if_newer_installs_only_a_verified_newer_release() {
    let skill = fake_skill("cmd-auto");
    let binary = skill.join("bin/linux-x86_64/callgraph");

    // nincs újabb / nincs kiadás: semmi nem települ, a latest az állapotba kerül
    let same = vec![
        (API, Ok(release_json("v0.1.0"))),
        (ASSET, Ok(release_zip(b"SAME"))),
        (SUMS, Ok(sums(&release_zip(b"SAME"), NAME))),
    ];
    assert!(
        auto_command(&skill, same)
            .update_if_newer()
            .unwrap()
            .is_none()
    );
    assert_eq!(read(binary.clone()), "OLD");
    let state = CheckState::load(&skill.join("update-check.json"));
    assert_eq!(state.latest.as_deref(), Some("0.1.0"));
    assert!(
        auto_command(&skill, vec![])
            .update_if_newer()
            .unwrap()
            .is_none()
    );

    // eltérő ellenőrzőösszeg vagy hiányzó SHA256SUMS: hiba, semmi nem települ (kihagyási lehetőség nincs)
    for sums_response in [
        Ok(sums(&release_zip(b"TAMPERED"), NAME)),
        Err(FetchError::NotFound),
    ] {
        let responses = vec![
            (API, Ok(release_json("v0.2.0"))),
            (ASSET, Ok(release_zip(b"NEW"))),
            (SUMS, sums_response),
        ];
        let e = auto_command(&skill, responses)
            .update_if_newer()
            .unwrap_err();
        assert_eq!(e.context, "checksum", "{}", e.message);
        assert_eq!(read(binary.clone()), "OLD");
    }

    // hálózati hiba: hiba (a hívó a régi verzióval folytatja)
    let offline = vec![(API, Err(FetchError::Other("offline".into())))];
    assert!(auto_command(&skill, offline).update_if_newer().is_err());
    assert_eq!(read(binary.clone()), "OLD");

    // újabb, egyező összeggel: telepít, a felhasználói adat marad
    let newer = vec![
        (API, Ok(release_json("v0.2.0"))),
        (ASSET, Ok(release_zip(b"NEW"))),
        (SUMS, Ok(sums(&release_zip(b"NEW"), NAME))),
    ];
    let (version, path) = auto_command(&skill, newer)
        .update_if_newer()
        .unwrap()
        .unwrap();
    assert_eq!(version, Version::parse("0.2.0").unwrap());
    assert_eq!(path, binary.canonicalize().unwrap());
    assert_eq!(read(binary), "NEW");
    assert_eq!(read(skill.join("lang")), "hu\n");
    let _ = std::fs::remove_dir_all(skill);
}
