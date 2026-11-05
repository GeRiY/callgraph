//! `callgraph set-lang [en|hu]` — a terminál-kimenet nyelve: argumentum nélkül kiírja az aktuálisat (és honnan jön),
//! argumentummal elmenti a skill-mappa `lang` fájljába, és a skill-utasítást (`SKILL.md`) is az érvényes nyelvűre
//! cseréli. A `CALLGRAPH_LANG` környezeti változó mindkettőt felülírja.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::i18n::{self, Lang, LangSource};
use crate::skill::{SkillFailure, SkillHome, finish, sync_skill_md};
use crate::{t, tf};

const SOURCE: &str = "callgraph set-lang";

pub struct LangCommand {
    args: Vec<String>,
    home: SkillHome,
    /// A `CALLGRAPH_LANG` értéke (teszthez injektálható).
    env_lang: Option<String>,
}

impl LangCommand {
    pub fn new(args: Vec<String>, home: SkillHome) -> Self {
        Self::with_env(args, home, std::env::var("CALLGRAPH_LANG").ok())
    }

    pub(crate) fn with_env(args: Vec<String>, home: SkillHome, env_lang: Option<String>) -> Self {
        Self {
            args,
            home,
            env_lang,
        }
    }

    fn show(&self, out: &mut dyn Write) -> std::result::Result<(), SkillFailure> {
        let (lang, source) = i18n::current();
        let from = match source {
            LangSource::Env => t!(
                "CALLGRAPH_LANG environment variable",
                "CALLGRAPH_LANG környezeti változó"
            ),
            LangSource::File => t!("saved setting", "elmentett beállítás"),
            LangSource::Default => t!("default", "alapértelmezés"),
        };
        writeln!(
            out,
            "{}",
            tf!("Language: {} ({from})", "Nyelv: {} ({from})", lang.code())
        )?;
        for l in Lang::ALL {
            let mark = if l == lang { "*" } else { " " };
            writeln!(out, "  {mark} {}  {}", l.code(), l.native_name())?;
        }
        writeln!(
            out,
            "{}",
            t!(
                "Change: callgraph set-lang <en|hu>  (or for one run: CALLGRAPH_LANG=<en|hu>)",
                "Váltás: callgraph set-lang <en|hu>  (vagy egy futásra: CALLGRAPH_LANG=<en|hu>)"
            )
        )?;
        Ok(())
    }

    fn set(&self, value: &str, out: &mut dyn Write) -> std::result::Result<(), SkillFailure> {
        let Some(lang) = Lang::parse(value) else {
            return Err(SkillFailure::usage(tf!(
                "unknown language: {value} (available: en, hu)",
                "ismeretlen nyelv: {value} (választható: en, hu)"
            )));
        };
        let file = self.home.lang_file();
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&file, format!("{}\n", lang.code()))?;
        // a visszajelzés már az új nyelven
        writeln!(
            out,
            "{}",
            lang.pick(
                format!("Language set: {} ({})", lang.code(), lang.native_name()),
                format!("Nyelv beállítva: {} ({})", lang.code(), lang.native_name()),
            )
        )?;
        // a skill-utasítás az érvényes nyelvet követi (CALLGRAPH_LANG › az imént írt lang fájl)
        let (effective, source) = i18n::resolve(self.env_lang.clone(), &self.home);
        if sync_skill_md(&self.home, effective)? {
            writeln!(
                out,
                "{}",
                lang.pick(
                    format!(
                        "Skill instructions (SKILL.md) switched to: {} — restart Claude Code to load them.",
                        effective.code()
                    ),
                    format!(
                        "A skill-utasítás (SKILL.md) nyelve: {} — a betöltéséhez indítsd újra a Claude Code-ot.",
                        effective.code()
                    ),
                )
            )?;
        }
        if source == LangSource::Env {
            writeln!(
                out,
                "{}",
                lang.pick(
                    "Note: the CALLGRAPH_LANG environment variable is set and overrides this setting.",
                    "Figyelem: a CALLGRAPH_LANG környezeti változó be van állítva, és felülírja ezt a beállítást.",
                )
            )?;
        }
        Ok(())
    }
}

impl Command for LangCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let result = match self.args.as_slice() {
            [] => self.show(out),
            [value] => self.set(value, out),
            _ => Err(SkillFailure::usage(t!(
                "usage: callgraph set-lang [en|hu]",
                "használat: callgraph set-lang [en|hu]"
            ))),
        };
        finish(&self.home, SOURCE, result, err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_writes_lang_file_and_rejects_unknown() {
        let dir = std::env::temp_dir().join(format!("callgraph-lang-cmd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let home = SkillHome::new(
            dir.clone(),
            dir.display().to_string(),
            dir.join("projects.json"),
        );
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = LangCommand::new(vec!["hu".into()], home.clone())
            .run(&mut out, &mut err)
            .unwrap();
        assert_eq!(code, 0);
        assert_eq!(std::fs::read_to_string(home.lang_file()).unwrap(), "hu\n");
        assert!(String::from_utf8_lossy(&out).starts_with("Nyelv beállítva: hu"));
        // nincs skill-i18n/ (fejlesztői checkout): a SKILL.md nem jön létre, és nincs róla sor
        assert!(!home.skill_md_file().exists());
        assert!(!String::from_utf8_lossy(&out).contains("SKILL.md"));

        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = LangCommand::new(vec!["de".into()], home.clone())
            .run(&mut out, &mut err)
            .unwrap();
        assert_eq!(code, 1);
        assert_eq!(std::fs::read_to_string(home.lang_file()).unwrap(), "hu\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn set_lang_switches_skill_md() {
        let dir = std::env::temp_dir().join(format!("callgraph-lang-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("skill-i18n")).unwrap();
        std::fs::write(
            dir.join("skill-i18n/SKILL.en.md"),
            "---\nname: callgraph\n---\nEN\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("skill-i18n/SKILL.hu.md"),
            "---\nname: callgraph\n---\nHU\n",
        )
        .unwrap();
        let home = SkillHome::new(
            dir.clone(),
            dir.display().to_string(),
            dir.join("projects.json"),
        );
        let skill = || std::fs::read_to_string(home.skill_md_file()).unwrap();
        let run = |arg: &str, env: Option<&str>| {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let code = LangCommand::with_env(vec![arg.into()], home.clone(), env.map(String::from))
                .run(&mut out, &mut err)
                .unwrap();
            assert_eq!(code, 0);
            String::from_utf8(out).unwrap()
        };
        let out = run("hu", None);
        assert!(
            skill().starts_with(
                "---\nname: callgraph\n---\n<!-- generated from skill-i18n/SKILL.hu.md"
            )
        );
        assert!(skill().ends_with("HU\n"));
        assert!(out.contains("SKILL.md") && out.contains("Claude Code"));
        run("en", None);
        assert!(skill().ends_with("EN\n"));
        // CALLGRAPH_LANG=en mellett a lang fájl hu lesz, de az érvényes nyelv (en) dönt
        run("hu", Some("en"));
        assert_eq!(std::fs::read_to_string(home.lang_file()).unwrap(), "hu\n");
        assert!(skill().ends_with("EN\n"));
        // argumentum nélkül: semmi nem változik
        let (mut out, mut err) = (Vec::new(), Vec::new());
        LangCommand::with_env(vec![], home.clone(), None)
            .run(&mut out, &mut err)
            .unwrap();
        assert!(skill().ends_with("EN\n"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
