//! Az `UpdateCheck` tesztjei ideiglenes skill-mappában, számláló hamis indítóval — valódi folyamat nélkül.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::*;

const NOW: u64 = 1_800_000_000;

/// Csak megszámolja az indításokat.
struct CountingSpawner(Rc<Cell<u32>>);

impl Spawner for CountingSpawner {
    fn spawn(&self) {
        self.0.set(self.0.get() + 1);
    }
}

fn temp_skill(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("callgraph-check-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn home(dir: &Path) -> SkillHome {
    SkillHome::new(
        dir.to_path_buf(),
        "/skill".into(),
        dir.join("projects.json"),
    )
}

fn check(dir: &Path, env: CheckEnv) -> (UpdateCheck, Rc<Cell<u32>>) {
    let spawned = Rc::new(Cell::new(0));
    let check = UpdateCheck::new(&home(dir), env, Box::new(CountingSpawner(spawned.clone())))
        .with_current(Version::parse("0.1.0").unwrap());
    (check, spawned)
}

fn on() -> CheckEnv {
    CheckEnv {
        setting: None,
        ci: false,
        in_skill: true,
    }
}

fn run(check: &UpdateCheck, now: u64, succeeded: bool) -> String {
    let mut out = Vec::new();
    check.after_command(now, succeeded, &mut out);
    String::from_utf8(out).unwrap()
}

#[test]
fn first_run_spawns_once_then_notices_once() {
    let dir = temp_skill("flow");
    let (check, spawned) = check(&dir, on());
    assert_eq!(run(&check, NOW, true), "");
    assert_eq!(spawned.get(), 1);
    // a checked_at azonnal beírva: a következő futás nem indít újat
    assert_eq!(CheckState::load(&dir.join(STATE_FILE)).checked_at, NOW);
    assert_eq!(run(&check, NOW + 1, true), "");
    assert_eq!(spawned.get(), 1);

    // a háttér-gyerek beírja a latest-et → egyszer szól, utána 24 óráig nem
    record_latest(
        &home(&dir),
        NOW + 2,
        Ok(Some(Version::parse("0.2.0").unwrap())),
    );
    assert_eq!(run(&check, NOW + 3, false), "");
    let line = run(&check, NOW + 4, true);
    assert_eq!(line.lines().count(), 1, "{line}");
    assert!(line.starts_with("ℹ callgraph 0.2.0"), "{line}");
    assert!(line.contains("0.1.0") && line.trim_end().ends_with("bash /skill/callgraph.sh update"));
    assert_eq!(run(&check, NOW + 5, true), "");

    // egy nap múlva: új ellenőrzés és újra értesítés
    let later = run(&check, NOW + INTERVAL_SECS + 10, true);
    assert!(later.starts_with("ℹ callgraph 0.2.0"), "{later}");
    assert_eq!(spawned.get(), 2);

    // hálózati hiba: a latest marad, csak az idő frissül
    record_latest(&home(&dir), NOW + 7, Err(()));
    let state = CheckState::load(&dir.join(STATE_FILE));
    assert_eq!(
        (state.checked_at, state.latest.as_deref()),
        (NOW + 7, Some("0.2.0"))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn disabled_check_touches_nothing() {
    let dir = temp_skill("off");
    let env = CheckEnv {
        setting: Some("0".into()),
        ..on()
    };
    let (check, spawned) = check(&dir, env);
    assert_eq!(run(&check, NOW, true), "");
    assert_eq!(spawned.get(), 0);
    assert!(!dir.join(STATE_FILE).exists());
    let _ = std::fs::remove_dir_all(dir);
}
