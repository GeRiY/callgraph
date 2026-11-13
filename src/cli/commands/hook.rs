//! `callgraph hook post-edit|install|uninstall|status|session-start|install-session|never <tétel>` — Claude
//! Code-hookok (`crate::hook`): fájl-írás utáni háttérfrissítés és session-indítási ajánlások.
//!
//! - `post-edit`: a hook maga. A stdin-en kapott `PostToolUse` JSON-ból a szerkesztett fájl; ha egy ismert projektben
//!   van (regisztrált alias-gyökér, vagy már létező `.callgraph/`) és kódfájl, leválasztva elindítja rá a
//!   `callgraph refresh`-t. Mindig azonnal, 0-val lép ki, és semmit sem ír ki (a hook kimenete a Claude Code-é).
//! - `install` / `uninstall` / `status`: a hook-bejegyzés a `settings.json`-ban (alapból `~/.claude/settings.json`);
//!   a többi beállítás és hook marad, írás előtt `settings.json.bak` mentés.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::cli::{Command, HookAction};
use crate::error::Result;
use crate::hook::session_start::{self as session, Commands, Facts, Item, State};
use crate::hook::{
    HOOK_EVENT, HOOK_MATCHER, POST_EDIT, SESSION_START, Settings, edited_file, hook_command,
    hook_command_for, install, install_for, is_installed, is_installed_for, project_root_for,
    uninstall_for,
};
use crate::io::FileWalker;
use crate::lang::LanguageRegistry;
use crate::skill::{ProjectRegistry, SkillHome};

/// A hook-parancs; a regisztrált projektek a `SkillHome` registryjéből.
pub struct HookCommand {
    action: HookAction,
    settings: Option<PathBuf>,
    home: SkillHome,
}

impl HookCommand {
    pub fn new(action: HookAction, settings: Option<PathBuf>, home: SkillHome) -> Self {
        Self {
            action,
            settings,
            home,
        }
    }

    /// A háttér-frissítés argumentumai a hook JSON-jából; `None`: nincs teendő (hibás / ismeretlen bemenet, nem
    /// ismert projekt, nem kódfájl).
    pub(crate) fn post_edit_args(&self, json: &str) -> Option<Vec<OsString>> {
        let file = edited_file(json)?;
        let registered: Vec<PathBuf> = ProjectRegistry::load(self.home.registry_file())
            .map(|registry| registry.roots())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|root| PathBuf::from(root).canonicalize().ok())
            .collect();
        let root = project_root_for(&file, &registered)?;
        let file = crate::graph::within_root(&root, &file)?;
        let registry = LanguageRegistry::with_defaults();
        if !FileWalker::for_registry(&registry).includes(&root, &file) {
            return None;
        }
        Some(vec![
            "refresh".into(),
            "--project".into(),
            root.into_os_string(),
            "--".into(),
            file.into_os_string(),
        ])
    }

    fn post_edit(&self) {
        let mut json = String::new();
        if std::io::stdin().read_to_string(&mut json).is_err() {
            return;
        }
        if let Some(args) = self.post_edit_args(&json) {
            crate::update::check::spawn_detached_as(args, "hook");
        }
    }

    fn settings(&self) -> Settings {
        Settings::locate(self.settings.clone())
    }

    fn install(&self, out: &mut dyn Write) -> Result<()> {
        let settings = self.settings();
        let exe = std::env::current_exe()?;
        let exe = exe.canonicalize().unwrap_or(exe);
        let command = hook_command(&exe);
        let shown = settings.file().display();
        match install(&settings.read()?, settings.file(), &command)? {
            None => writeln!(
                out,
                "{}",
                crate::tf!(
                    "hook: already installed ({shown})",
                    "hook: már telepítve ({shown})"
                )
            )?,
            Some(text) => {
                let backup = settings.write(&text)?;
                writeln!(
                    out,
                    "{}",
                    crate::tf!(
                        "hook: installed in {shown} ({HOOK_EVENT} {HOOK_MATCHER} → background callgraph refresh); remove: callgraph hook uninstall",
                        "hook: telepítve ide: {shown} ({HOOK_EVENT} {HOOK_MATCHER} → callgraph refresh a háttérben); eltávolítás: callgraph hook uninstall"
                    )
                )?;
                if backup {
                    let saved = settings.backup_file();
                    writeln!(
                        out,
                        "{}",
                        crate::tf!("backup: {}", "biztonsági mentés: {}", saved.display())
                    )?;
                }
            }
        }
        Ok(())
    }

    /// A `SessionStart` hook felvétele (idempotens, `.bak` mentéssel).
    fn install_session(&self, out: &mut dyn Write) -> Result<()> {
        let settings = self.settings();
        let exe = std::env::current_exe()?;
        let exe = exe.canonicalize().unwrap_or(exe);
        let command = hook_command_for(&exe, &SESSION_START);
        let shown = settings.file().display();
        match install_for(&settings.read()?, settings.file(), &command, &SESSION_START)? {
            None => writeln!(
                out,
                "{}",
                crate::tf!(
                    "session hook: already installed ({shown})",
                    "session-hook: már telepítve ({shown})"
                )
            )?,
            Some(text) => {
                settings.write(&text)?;
                writeln!(
                    out,
                    "{}",
                    crate::tf!(
                        "session hook: installed in {shown} (SessionStart → recommended settings and update notice); remove: callgraph hook uninstall",
                        "session-hook: telepítve ide: {shown} (SessionStart → ajánlott beállítások és frissítés-értesítés); eltávolítás: callgraph hook uninstall"
                    )
                )?;
            }
        }
        Ok(())
    }

    /// Mindkét saját hook kivétele, egy írással.
    fn uninstall(&self, out: &mut dyn Write) -> Result<()> {
        let settings = self.settings();
        let shown = settings.file().display();
        let text = settings.read()?;
        let after_edit = uninstall_for(&text, settings.file(), &POST_EDIT)?;
        let base = after_edit.clone().unwrap_or_else(|| text.clone());
        let after_session = uninstall_for(&base, settings.file(), &SESSION_START)?;
        let result = after_session.or(after_edit);
        match result {
            None => writeln!(
                out,
                "{}",
                crate::tf!(
                    "hook: not installed ({shown})",
                    "hook: nincs telepítve ({shown})"
                )
            )?,
            Some(text) => {
                settings.write(&text)?;
                let saved = settings.backup_file();
                writeln!(
                    out,
                    "{}",
                    crate::tf!(
                        "hook: removed from {shown} (backup: {})",
                        "hook: eltávolítva innen: {shown} (biztonsági mentés: {})",
                        saved.display()
                    )
                )?;
            }
        }
        Ok(())
    }

    fn status(&self, out: &mut dyn Write) -> Result<()> {
        let settings = self.settings();
        let shown = settings.file().display();
        let text = settings.read()?;
        let line = if is_installed(&text, settings.file())? {
            crate::tf!("hook: installed ({shown})", "hook: telepítve ({shown})")
        } else {
            crate::tf!(
                "hook: not installed ({shown}); add: callgraph hook install",
                "hook: nincs telepítve ({shown}); telepítés: callgraph hook install"
            )
        };
        writeln!(out, "{line}")?;
        let line = if is_installed_for(&text, settings.file(), &SESSION_START)? {
            crate::tf!(
                "session hook: installed ({shown})",
                "session-hook: telepítve ({shown})"
            )
        } else {
            crate::tf!(
                "session hook: not installed ({shown}); add: callgraph hook install-session",
                "session-hook: nincs telepítve ({shown}); telepítés: callgraph hook install-session"
            )
        };
        writeln!(out, "{line}")?;
        Ok(())
    }

    /// A parancsok a felhasználónak / agentnek (a valódi utakkal).
    fn commands(&self) -> Commands {
        let exe = std::env::current_exe().unwrap_or_default();
        let exe = exe.canonicalize().unwrap_or(exe);
        let quoted = hook_command_for(&exe, &SESSION_START);
        let quoted = quoted
            .strip_suffix(SESSION_START.marker)
            .unwrap_or(&quoted)
            .trim_end()
            .to_owned();
        let script = self.home.script("callgraph.sh");
        Commands {
            mcp_register: format!(
                "claude mcp remove -s user callgraph >/dev/null 2>&1; claude mcp add -s user callgraph -- {quoted} mcp --auto-update"
            ),
            hook_install: format!("{quoted} hook install"),
            exe: quoted,
            current: crate::update::Version::current().to_string(),
            update: format!("{script} update"),
            telemetry_on: format!("{script} telemetry on"),
            telemetry_preview: format!("{script} telemetry preview"),
        }
    }

    /// A `SessionStart` hook: csak akkor ír ki valamit (JSON), ha van ajánlás vagy frissítés-értesítés.
    fn session_start(&self, out: &mut dyn Write) -> Result<()> {
        let get = |k: &str| std::env::var(k).ok();
        if session::disabled(get) {
            return Ok(());
        }
        // a hook stdin-jét (a session adatai) nem használjuk, de kiolvassuk, hogy a hívó ne akadjon el
        let mut ignored = String::new();
        let _ = std::io::stdin().read_to_string(&mut ignored);
        let now = crate::update::check::now_secs();
        let today = crate::update::usage::day_number(now);
        let settings = self.settings();
        let hook_installed = settings
            .read()
            .ok()
            .and_then(|text| is_installed(&text, settings.file()).ok())
            .unwrap_or(true);
        let claude_json = claude_json_path().and_then(|p| std::fs::read_to_string(p).ok());
        let mcp = session::mcp_state(claude_json.as_deref(), on_path("claude"));
        let off = |k: &str| {
            get(k).is_some_and(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "0" | "off" | "false" | "no"
                )
            })
        };
        let telemetry_open = !off("CALLGRAPH_TELEMETRY")
            && !off("CALLGRAPH_STATS")
            && crate::stats::telemetry::Telemetry::from_env(&self.home)
                .consent()
                .open();
        // a meglévő napi frissítés-ellenőrzés: szükség esetén háttérben indul; ha újabbról szólni kell, a sor nem üres
        let mut notice = Vec::new();
        crate::update::check::UpdateCheck::from_env(&self.home).after_command(
            now,
            true,
            &mut notice,
        );
        let update = (!notice.is_empty())
            .then(|| crate::update::check::CheckState::load(&self.home.update_check_file()).latest)
            .flatten();
        let facts = Facts {
            hook_installed,
            mcp,
            telemetry_open,
            update,
        };
        let file = self.home.dir().join(session::STATE_FILE);
        let state = State::load(&file);
        let (items, next) = session::decide(&state, &facts, today);
        if next != state && next.save(&file).is_err() {
            // mentés nélkül nem szólunk (különben minden sessionnél ugyanazt mondanánk)
            return Ok(());
        }
        if let Some(json) = session::render(&items, &facts, &self.commands()) {
            writeln!(out, "{json}")?;
        }
        Ok(())
    }

    /// Egy ajánlás végleges elhallgattatása.
    fn never(&self, item: &str, out: &mut dyn Write) -> Result<()> {
        let Some(item) = Item::parse(item) else {
            return Err(crate::error::CallgraphError::Usage(
                crate::t!(
                    "usage: callgraph hook never hook|mcp|telemetry",
                    "használat: callgraph hook never hook|mcp|telemetry"
                )
                .into(),
            ));
        };
        let file = self.home.dir().join(session::STATE_FILE);
        let mut state = State::load(&file);
        state.never(item);
        state.save(&file)?;
        writeln!(
            out,
            "{}",
            crate::tf!(
                "callgraph will not recommend this again: {}",
                "a callgraph ezt többé nem ajánlja: {}",
                item.key()
            )
        )?;
        Ok(())
    }
}

impl HookCommand {
    /// Az ajánlások visszaállítása (egy tétel, vagy mindegyik).
    fn reset(&self, item: Option<&str>, out: &mut dyn Write) -> Result<()> {
        let item = match item {
            None => None,
            Some(text) => Some(Item::parse(text).ok_or_else(|| {
                crate::error::CallgraphError::Usage(
                    crate::t!(
                        "usage: callgraph hook reset [hook|mcp|telemetry]",
                        "használat: callgraph hook reset [hook|mcp|telemetry]"
                    )
                    .into(),
                )
            })?),
        };
        let file = self.home.dir().join(session::STATE_FILE);
        let mut state = State::load(&file);
        state.reset(item);
        state.save(&file)?;
        writeln!(
            out,
            "{}",
            crate::tf!(
                "callgraph will recommend it again when it is off: {}",
                "a callgraph újra ajánlja, ha ki van kapcsolva: {}",
                item.map_or("all", Item::key)
            )
        )?;
        Ok(())
    }
}

/// A Claude Code felhasználói állapotfájlja (benne a user-scope `mcpServers`): `$CLAUDE_CONFIG_DIR/.claude.json`
/// › `~/.claude.json`.
fn claude_json_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir).join(".claude.json"));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".claude.json"))
}

/// Elérhető-e a parancs a `PATH`-on (Windowson `.exe` / `.cmd` is).
fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        ["", ".exe", ".cmd"]
            .iter()
            .any(|ext| dir.join(format!("{name}{ext}")).is_file())
    })
}

impl Command for HookCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        match self.action {
            HookAction::PostEdit => self.post_edit(),
            HookAction::Install => self.install(out)?,
            HookAction::Uninstall => self.uninstall(out)?,
            HookAction::Status => self.status(out)?,
            HookAction::SessionStart => self.session_start(out)?,
            HookAction::InstallSession => self.install_session(out)?,
            HookAction::Never(ref item) => self.never(item, out)?,
            HookAction::Reset(ref item) => self.reset(item.as_deref(), out)?,
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn payload(file: &Path) -> String {
        serde_json::json!({ "tool_input": { "file_path": file } }).to_string()
    }

    #[test]
    fn post_edit_refreshes_code_files_of_known_projects_only() {
        let base = std::env::temp_dir().join(format!("callgraph-hookcmd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("proj/.callgraph")).unwrap();
        fs::create_dir_all(base.join("proj/src")).unwrap();
        fs::create_dir_all(base.join("plain")).unwrap();
        let base = base.canonicalize().unwrap();
        let root = base.join("proj");
        let code = root.join("src/a.php");
        fs::write(&code, "<?php\n").unwrap();
        let home = SkillHome::new(base.clone(), String::new(), base.join("projects.json"));
        let command = HookCommand::new(HookAction::PostEdit, None, home);
        assert_eq!(
            command.post_edit_args(&payload(&code)),
            Some(vec![
                OsString::from("refresh"),
                OsString::from("--project"),
                root.clone().into_os_string(),
                OsString::from("--"),
                code.clone().into_os_string(),
            ])
        );
        // nem kódfájl, a tároló maga, nem ismert projekt, hibás bemenet: semmi
        assert_eq!(
            command.post_edit_args(&payload(&root.join("README.txt"))),
            None
        );
        assert_eq!(
            command.post_edit_args(&payload(&root.join(".callgraph/notes.jsonl"))),
            None
        );
        assert_eq!(
            command.post_edit_args(&payload(&base.join("plain/b.php"))),
            None
        );
        assert_eq!(command.post_edit_args("{"), None);
        let _ = fs::remove_dir_all(&base);
    }
}
