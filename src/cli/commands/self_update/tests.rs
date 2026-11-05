//! A `self-update` parancs tesztjei hamis letöltővel (DI) — hálózat nélkül.

use std::collections::HashMap;
use std::path::Path;

use super::*;
use crate::update::tests::{fake_skill, read, release_zip, target_of, test_platform};

const API: &str = "https://example.invalid/releases/latest";
const ASSET: &str = "https://example.invalid/download/callgraph-linux-x86_64.zip";

/// URL → válasz; ami nincs benne, az 404.
struct FakeFetcher(HashMap<String, std::result::Result<Vec<u8>, FetchError>>);

impl Fetcher for FakeFetcher {
    fn get(&self, url: &str, _limit: u64) -> std::result::Result<Vec<u8>, FetchError> {
        self.0
            .get(url)
            .cloned()
            .unwrap_or(Err(FetchError::NotFound))
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

/// A parancs futtatása: (kilépési kód, stdout, stderr).
fn run(
    args: &[&str],
    skill: &Path,
    responses: Vec<(&str, std::result::Result<Vec<u8>, FetchError>)>,
    in_skill: bool,
) -> (i32, String, String) {
    let env = UpdateEnv {
        fetcher: Box::new(FakeFetcher(
            responses
                .into_iter()
                .map(|(u, r)| (u.to_owned(), r))
                .collect(),
        )),
        api: API.to_owned(),
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
