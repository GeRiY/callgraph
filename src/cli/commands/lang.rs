//! `callgraph set-lang [en|hu]` — a terminál-kimenet nyelve: argumentum nélkül kiírja az aktuálisat (és honnan jön),
//! argumentummal elmenti a skill-mappa `lang` fájljába. A `CALLGRAPH_LANG` környezeti változó mindkettőt felülírja.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::i18n::{self, Lang, LangSource};
use crate::skill::{SkillFailure, SkillHome, finish};
use crate::{t, tf};

const SOURCE: &str = "callgraph set-lang";

pub struct LangCommand {
    args: Vec<String>,
    home: SkillHome,
}

impl LangCommand {
    pub fn new(args: Vec<String>, home: SkillHome) -> Self {
        Self { args, home }
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
        if let (_, LangSource::Env) = i18n::current() {
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

        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = LangCommand::new(vec!["de".into()], home.clone())
            .run(&mut out, &mut err)
            .unwrap();
        assert_eq!(code, 1);
        assert_eq!(std::fs::read_to_string(home.lang_file()).unwrap(), "hu\n");
        let _ = std::fs::remove_dir_all(dir);
    }
}
