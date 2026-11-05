//! `callgraph project add|list|remove|resolve|get|set-config` — a `projects.sh` (jq) utódja.

use std::io::Write;
use std::path::Path;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::{ProjectRegistry, SkillFailure, SkillHome, finish};

type Outcome = std::result::Result<(), SkillFailure>;

const SOURCE: &str = "projects.sh";
const ADD_USAGE: &str =
    "Használat: bash ~/.claude/skills/callgraph/projects.sh add <alias> <abszolút_útvonal>";

pub struct ProjectCommand {
    args: Vec<String>,
    home: SkillHome,
}

impl ProjectCommand {
    pub fn new(args: Vec<String>, home: SkillHome) -> Self {
        Self { args, home }
    }

    fn arg(&self, i: usize) -> &str {
        self.args.get(i).map_or("", String::as_str)
    }

    fn exec(&self, out: &mut dyn Write) -> Outcome {
        let file = self.home.registry_file();
        ProjectRegistry::ensure_file(file)?;
        let mut reg = ProjectRegistry::load(file)?;
        match self.arg(0) {
            "add" => self.add(&mut reg, out),
            "remove" => {
                let alias = self.arg(1);
                if alias.is_empty() {
                    return Err(SkillFailure::usage(
                        "Használat: bash ~/.claude/skills/callgraph/projects.sh remove <alias>",
                    ));
                }
                reg.remove(alias);
                reg.save()?;
                writeln!(out, "eltávolítva: {alias}")?;
                Ok(())
            }
            "resolve" => {
                let alias = self.arg(1);
                let root = reg.root(alias).ok_or_else(|| {
                    SkillFailure::not_found(format!(
                        "Ismeretlen projekt-alias: {alias} (lásd: bash ~/.claude/skills/callgraph/projects.sh list)"
                    ))
                })?;
                writeln!(out, "{root}")?;
                Ok(())
            }
            "get" => {
                if let Some(v) = reg.setting(self.arg(1), self.arg(2)) {
                    writeln!(out, "{v}")?;
                }
                Ok(())
            }
            "set-config" => self.set_config(&mut reg, out),
            "list" | "" => {
                for line in reg.list_lines() {
                    writeln!(out, "{line}")?;
                }
                Ok(())
            }
            other => Err(SkillFailure::usage(format!(
                "Ismeretlen parancs: {other} (add|list|remove|resolve|get|set-config)"
            ))),
        }
    }

    fn add(&self, reg: &mut ProjectRegistry, out: &mut dyn Write) -> Outcome {
        let (alias, path) = (self.arg(1), self.arg(2));
        if alias.is_empty() || path.is_empty() {
            return Err(SkillFailure::usage(ADD_USAGE));
        }
        if !Path::new(path).is_absolute() {
            return Err(SkillFailure::usage(format!(
                "Az útvonalnak abszolútnak kell lennie (/-vel kezdődjön): {path}"
            )));
        }
        if !Path::new(path).is_dir() {
            return Err(SkillFailure::not_found(format!("Nem létező mappa: {path}")));
        }
        reg.add(alias, path);
        reg.save()?;
        writeln!(out, "hozzáadva: {alias} -> {path}")?;
        Ok(())
    }

    fn set_config(&self, reg: &mut ProjectRegistry, out: &mut dyn Write) -> Outcome {
        let alias = self.arg(1);
        if alias.is_empty() {
            return Err(SkillFailure::usage(
                "Használat: bash ~/.claude/skills/callgraph/projects.sh set-config <alias> [--phpstan-level N] [--phpstan-config <út>]",
            ));
        }
        if !reg.contains(alias) {
            return Err(SkillFailure::not_found(format!(
                "Nincs '{alias}' regisztrált projekt — előbb: bash projects.sh add {alias} <abszolút_útvonal>"
            )));
        }
        let (mut level, mut config) = (None, None);
        let mut rest = self.args[2..].iter();
        while let Some(flag) = rest.next() {
            let value = rest.next().map(String::as_str);
            match flag.as_str() {
                "--phpstan-level" => level = value,
                "--phpstan-config" => config = value,
                other => {
                    return Err(SkillFailure::usage(format!(
                        "Ismeretlen kapcsoló: {other} (várt: --phpstan-level | --phpstan-config)"
                    )));
                }
            }
        }
        let (level, config) = (
            level.filter(|v| !v.is_empty()),
            config.filter(|v| !v.is_empty()),
        );
        if level.is_none() && config.is_none() {
            return Err(SkillFailure::usage(
                "Adj meg legalább egyet: --phpstan-level N | --phpstan-config <út>",
            ));
        }
        let entry = reg
            .set_config(alias, level, config)
            .cloned()
            .unwrap_or_default();
        reg.save()?;
        writeln!(out, "beállítva: {alias}")?;
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&entry).map_err(crate::error::CallgraphError::from)?
        )?;
        Ok(())
    }
}

impl Command for ProjectCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let result = self.exec(out);
        finish(&self.home, SOURCE, result, err)
    }
}
