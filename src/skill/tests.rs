use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::clock::{iso_from_secs, new_id};
use super::*;

static N: AtomicUsize = AtomicUsize::new(0);

/// Ideiglenes skill-mappa; a végén törlődik.
struct Tmp(PathBuf);
impl Tmp {
    fn new() -> Self {
        let n = N.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("callgraph-skill-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn home(&self) -> SkillHome {
        SkillHome::new(self.0.clone(), "/S".into(), self.0.join("projects.json"))
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn iso_dates_and_ids() {
    assert_eq!(iso_from_secs(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso_from_secs(1_793_000_000), "2026-10-26T07:33:20Z");
    assert_eq!(iso_from_secs(951_782_400), "2000-02-29T00:00:00Z");
    let (a, b) = (new_id(), new_id());
    assert_eq!(a.len(), 8);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(a, b);
}

#[test]
fn registry_keeps_order_and_both_entry_forms() {
    let t = Tmp::new();
    let f = t.0.join("projects.json");
    std::fs::write(
        &f,
        r#"{"z": {"path": "/p/z", "phpstan_level": "3"}, "a": "/p/a"}"#,
    )
    .unwrap();
    let mut r = ProjectRegistry::load(&f).unwrap();
    assert_eq!(r.root("z").as_deref(), Some("/p/z"));
    assert_eq!(r.root("a").as_deref(), Some("/p/a"));
    assert_eq!(r.setting("z", "phpstan_level").as_deref(), Some("3"));
    assert_eq!(r.setting("a", "phpstan_level"), None);
    r.add("m", "/p/m");
    r.set_config("a", None, Some("cfg.neon"));
    r.save().unwrap();
    let r = ProjectRegistry::load(&f).unwrap();
    assert_eq!(
        r.list_lines(),
        vec![
            "z\t/p/z  [phpstan_level=3 phpstan_config=-]",
            "a\t/p/a  [phpstan_level=- phpstan_config=cfg.neon]",
            "m\t/p/m",
        ]
    );
    let text = std::fs::read_to_string(&f).unwrap();
    assert!(
        text.starts_with("{\n  \"z\": {\n    \"path\": \"/p/z\""),
        "{text}"
    );
}

#[test]
fn registry_remove_and_invalid_json() {
    let t = Tmp::new();
    let f = t.0.join("projects.json");
    ProjectRegistry::ensure_file(&f).unwrap();
    let mut r = ProjectRegistry::load(&f).unwrap();
    r.add("a", "/p/a");
    assert!(r.remove("a"));
    assert!(!r.remove("a"));
    std::fs::write(&f, "nem json").unwrap();
    assert!(ProjectRegistry::load(&f).is_err());
}

#[test]
fn fail_logs_and_prints_feedback_line() {
    let t = Tmp::new();
    let home = t.home();
    let mut err = Vec::new();
    let code = fail(
        &home,
        "callgraph.sh",
        "usage",
        "lásd: bash ~/.claude/skills/callgraph/x.sh",
        &mut err,
    )
    .unwrap();
    assert_eq!(code, 1);
    let text = String::from_utf8(err).unwrap();
    let rows = ErrorLog::new(home.errors_file()).all();
    assert_eq!(rows.len(), 1);
    let id = &rows[0].id;
    assert_eq!(rows[0].message, "lásd: bash /S/x.sh");
    assert_eq!(
        text,
        format!(
            "lásd: bash /S/x.sh\n[error-id: {id}]  report: https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml  (details to paste: bash /S/feedback.sh show {id})\n"
        )
    );
    let line = std::fs::read_to_string(home.errors_file()).unwrap();
    assert!(
        line.starts_with(&format!("{{\"id\":\"{id}\",\"ts\":\"")),
        "{line}"
    );
}

#[test]
fn user_errors_are_not_logged_internal_errors_are() {
    let t = Tmp::new();
    let home = t.home();
    for failure in [
        SkillFailure::usage("Unknown layer: nope"),
        SkillFailure::not_found("Not found: x.php"),
        SkillFailure::from(crate::error::CallgraphError::Usage("bad flag".into())),
    ] {
        let mut err = Vec::new();
        let code = finish(&home, "callgraph.sh", Err(failure), &mut err).unwrap();
        assert_eq!(code, 1);
        let text = String::from_utf8(err).unwrap();
        assert!(!text.contains("error-id"), "{text}");
        assert_eq!(text.lines().count(), 1, "{text}");
    }
    assert!(!home.errors_file().exists());
    let mut err = Vec::new();
    let failure = SkillFailure::from(std::io::Error::other("disk full"));
    let code = finish(&home, "callgraph.sh", Err(failure), &mut err).unwrap();
    assert_eq!(code, 1);
    assert!(String::from_utf8(err).unwrap().contains("[error-id: "));
    assert_eq!(ErrorLog::new(home.errors_file()).all().len(), 1);
}

#[test]
fn feedback_add_resolve_list_show() {
    let t = Tmp::new();
    let home = t.home();
    let errors = ErrorLog::new(home.errors_file());
    let eid = errors.log("callgraph.sh", "not_found", "Nem található: x");
    let store = FeedbackStore::new(home.feedback_file());
    let a = store.add(Some(&eid), "várt: y").unwrap();
    let b = store.add(None, "ötlet").unwrap();
    assert!(store.resolve(&b).unwrap());
    assert!(!store.resolve("nincs").unwrap());
    let all = store.list(false, &errors);
    assert_eq!(all.len(), 7);
    assert!(
        all[0].ends_with(&format!("id:{a}  [OPEN]  error:{eid}")),
        "{}",
        all[0]
    );
    assert_eq!(all[2], "  error: (callgraph.sh/not_found) Nem található: x");
    assert!(
        all[4].contains("[RESOLVED 20") && all[4].ends_with("(no error)"),
        "{}",
        all[4]
    );
    assert_eq!(store.list(true, &errors).len(), 4);
    let raw = store.raw_for_error(&eid);
    assert_eq!(raw.len(), 1);
    assert!(
        raw[0].contains("\"error_id\":\"")
            && raw[0].ends_with("\"resolved\":false,\"resolved_at\":null}"),
        "{}",
        raw[0]
    );
    assert_eq!(errors.raw_matching(&eid).len(), 1);
}
