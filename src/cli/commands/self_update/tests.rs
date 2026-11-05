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

#[test]
fn redirect_source_finds_and_installs_the_latest_release() {
    let skill = fake_skill("cmd-redirect");
    let tag = "https://example.invalid/o/callgraph/releases/tag/v0.2.0";
    let zip =
        "https://example.invalid/o/callgraph/releases/download/v0.2.0/callgraph-linux-x86_64.zip";
    let redirect_key = format!("redirect:{WEB}");
    let source = || Box::new(RedirectSource::new(WEB, &test_platform()));
    let responses = || {
        vec![
            (redirect_key.as_str(), Ok(tag.as_bytes().to_vec())),
            (zip, Ok(release_zip(b"WEB"))),
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
