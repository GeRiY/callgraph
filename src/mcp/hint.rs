//! A beküldés-tipp az MCP-ben (mint az új-verzió értesítés): munkamenetenként legfeljebb egyszer, az első sikeres
//! eszköz-eredmény végén, ha a `stats::telemetry::Telemetry::agent_hint` szerint esedékes (ritka, a felhasználó
//! dönt). Mellette a beküldés időzítése: induláskor és óránként egy esedékességi vizsgálat (`maybe_spawn`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::params::{GenerateParams, RenameParams};
use super::request::ShowQuery;
use super::runner::QueryRunner;
use crate::skill::SkillHome;
use crate::stats::telemetry::{Telemetry, now_secs};

/// A végrehajtó a tippel.
struct HintRunner {
    inner: Arc<dyn QueryRunner>,
    home: SkillHome,
    pending: AtomicBool,
}

impl HintRunner {
    fn append(&self, mut text: String) -> String {
        if !self.pending.swap(false, Ordering::AcqRel) {
            return text;
        }
        if let Some(line) = Telemetry::from_env(&self.home).agent_hint(now_secs()) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&line);
            text.push('\n');
        }
        text
    }
}

impl QueryRunner for HintRunner {
    fn run(&self, query: &ShowQuery) -> Result<String, String> {
        self.inner.run(query).map(|text| self.append(text))
    }

    fn projects(&self) -> Result<String, String> {
        self.inner.projects().map(|text| self.append(text))
    }

    fn profile(&self, project: Option<&str>, full: bool, packages: bool) -> Result<String, String> {
        self.inner
            .profile(project, full, packages)
            .map(|text| self.append(text))
    }

    fn uses(&self, project: Option<&str>, package: &str, calls: bool) -> Result<String, String> {
        self.inner
            .uses(project, package, calls)
            .map(|text| self.append(text))
    }

    fn deps(&self, project: Option<&str>) -> Result<String, String> {
        self.inner.deps(project).map(|text| self.append(text))
    }

    fn startup_note(&self) -> String {
        self.inner.startup_note()
    }

    fn next(&self, project: Option<&str>, id: &str, choices: &[usize]) -> Result<String, String> {
        self.inner
            .next(project, id, choices)
            .map(|text| self.append(text))
    }

    fn generate(&self, params: &GenerateParams) -> Result<String, String> {
        self.inner.generate(params)
    }

    fn rename(&self, params: &RenameParams) -> Result<String, String> {
        self.inner.rename(params)
    }
}

/// A szerver végrehajtója a tippel; tiltott telemetriánál (CI, `CALLGRAPH_STATS=0`, `CALLGRAPH_TELEMETRY=0`)
/// változatlan. Elindítja az óránkénti beküldés-vizsgálatot is.
pub(crate) fn with_hint(runner: Arc<dyn QueryRunner>, home: &SkillHome) -> Arc<dyn QueryRunner> {
    if !Telemetry::from_env(home).env().allowed() {
        return runner;
    }
    let scheduled = home.clone();
    let _ = std::thread::Builder::new()
        .name("callgraph-telemetry".into())
        .spawn(move || {
            loop {
                Telemetry::from_env(&scheduled).maybe_spawn(now_secs());
                std::thread::sleep(Duration::from_secs(60 * 60));
            }
        });
    Arc::new(HintRunner {
        inner: runner,
        home: home.clone(),
        pending: AtomicBool::new(true),
    })
}
