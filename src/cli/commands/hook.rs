//! `callgraph hook post-edit|install|uninstall|status` — Claude Code-hook a fájl-írásokra (`crate::hook`).
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
use crate::hook::{
    HOOK_EVENT, HOOK_MATCHER, Settings, edited_file, hook_command, install, is_installed,
    project_root_for, uninstall,
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

    fn uninstall(&self, out: &mut dyn Write) -> Result<()> {
        let settings = self.settings();
        let shown = settings.file().display();
        match uninstall(&settings.read()?, settings.file())? {
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
        let line = if is_installed(&settings.read()?, settings.file())? {
            crate::tf!("hook: installed ({shown})", "hook: telepítve ({shown})")
        } else {
            crate::tf!(
                "hook: not installed ({shown}); add: callgraph hook install",
                "hook: nincs telepítve ({shown}); telepítés: callgraph hook install"
            )
        };
        writeln!(out, "{line}")?;
        Ok(())
    }
}

impl Command for HookCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        match self.action {
            HookAction::PostEdit => self.post_edit(),
            HookAction::Install => self.install(out)?,
            HookAction::Uninstall => self.uninstall(out)?,
            HookAction::Status => self.status(out)?,
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
