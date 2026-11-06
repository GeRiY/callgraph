//! A skill-mappa helye: itt él a `projects.json`, az `errors.jsonl`, a `feedback.jsonl`, az `update-check.json`
//! (felhasználói döntés) és a `stats/` (helyi használati statisztika).

use std::path::{Path, PathBuf};

/// Ezt a szöveget cseréljük a megjelenített skill-útra (a bash `fail` `~/.claude/skills/callgraph` cseréje).
const CANONICAL: &str = "~/.claude/skills/callgraph";

/// A skill adatmappája, a hibaüzenetekben megjelenített útja és a projekt-registry.
#[derive(Debug, Clone)]
pub struct SkillHome {
    dir: PathBuf,
    display: String,
    registry: PathBuf,
}

impl SkillHome {
    pub fn new(dir: PathBuf, display: String, registry: PathBuf) -> Self {
        Self {
            dir,
            display,
            registry,
        }
    }

    /// Környezetből: `CALLGRAPH_SKILL_DIR` (a burok adja) › a bináris helye (`<skill>/bin/[<os>-<arch>/]callgraph`)
    /// › `~/.claude/skills/callgraph`. Megjelenítés: `PHP_AST_HOME` › a mappa; registry: `CALLGRAPH_REGISTRY` › `<mappa>/projects.json`.
    pub fn from_env() -> Self {
        let dir = non_empty_env("CALLGRAPH_SKILL_DIR")
            .map(PathBuf::from)
            .or_else(from_exe)
            .unwrap_or_else(default_dir);
        let display = non_empty_env("PHP_AST_HOME").unwrap_or_else(|| dir.display().to_string());
        let registry = non_empty_env("CALLGRAPH_REGISTRY")
            .map(PathBuf::from)
            .unwrap_or_else(|| dir.join("projects.json"));
        Self::new(dir, display, registry)
    }

    pub fn errors_file(&self) -> PathBuf {
        self.dir.join("errors.jsonl")
    }

    pub fn feedback_file(&self) -> PathBuf {
        self.dir.join("feedback.jsonl")
    }

    /// Az automatikus frissítés-ellenőrzés állapota (`update::check`).
    pub fn update_check_file(&self) -> PathBuf {
        self.dir.join("update-check.json")
    }

    /// A helyi használati statisztika napi összesítői (`stats`; nem küldjük sehova).
    pub fn stats_dir(&self) -> PathBuf {
        self.dir.join("stats")
    }

    /// A kiválasztott nyelv (`en` / `hu`) — a `callgraph set-lang <en|hu>` írja.
    pub fn lang_file(&self) -> PathBuf {
        self.dir.join("lang")
    }

    /// A skill-mappa.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A Claude Code által betöltött skill-utasítás (`<skill>/SKILL.md`) — a `skill-i18n/SKILL.<nyelv>.md` másolata.
    pub fn skill_md_file(&self) -> PathBuf {
        self.dir.join("SKILL.md")
    }

    /// A skill-utasítás forrása egy nyelvre: `<skill>/skill-i18n/SKILL.<nyelv>.md`.
    pub fn skill_md_source(&self, lang: crate::i18n::Lang) -> PathBuf {
        self.dir
            .join("skill-i18n")
            .join(format!("SKILL.{}.md", lang.code()))
    }

    pub fn registry_file(&self) -> &Path {
        &self.registry
    }

    /// A hibaüzenetekben a skill-út (`~/.claude/skills/callgraph` helyett a ténylegesen megjelenítendő).
    pub fn localize(&self, text: &str) -> String {
        text.replace(CANONICAL, &self.display)
    }

    /// `bash <skill>/<script> ` — a súgó- és javaslat-sorok eleje.
    pub fn script(&self, name: &str) -> String {
        format!("bash {}/{name}", self.display)
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// A bináris fölötti első `bin` mappa szülője (legfeljebb két szint: `bin/callgraph`, `bin/<os>-<arch>/callgraph`).
fn from_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors()
        .skip(1)
        .take(2)
        .find(|p| p.file_name().is_some_and(|n| n == "bin"))
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

fn default_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".claude/skills/callgraph")
}
