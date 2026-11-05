//! A skill-utasítás (`<skill>/SKILL.md`) nyelve a kimenet nyelvét követi: a `skill-i18n/SKILL.<nyelv>.md` forrásból
//! készül (frontmatter, utána egy „generált” megjegyzés-sor, majd a törzs). Írja: `callgraph set-lang <nyelv>`,
//! `callgraph skill-sync` (a telepítő) és a sikeres `self-update`. Forrás nélkül (fejlesztői checkout) csendben semmit
//! sem tesz.

use std::io;
use std::path::Path;

use super::SkillHome;
use crate::i18n::{self, Lang};

/// A generált SKILL.md: a frontmatter (`---` … `---`) után egy megjegyzés-sor, hogy a forrást kell szerkeszteni
/// (a frontmatter marad elöl — a Claude Code azt olvassa).
pub fn render_skill_md(source: &str, lang: Lang) -> String {
    let note = format!(
        "<!-- generated from skill-i18n/SKILL.{}.md by callgraph set-lang / skill-sync; edit the sources -->\n",
        lang.code()
    );
    // a Windowson CRLF-esre mentett forrás is működjön (a frontmatter-keresés és a kimenet LF)
    let source = source.replace("\r\n", "\n");
    let source = source.as_str();
    let split = source
        .strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---\n"))
        .map(|end| "---\n".len() + end + "\n---\n".len());
    match split {
        Some(at) => format!("{}{note}{}", &source[..at], &source[at..]),
        None => format!("{note}{source}"),
    }
}

/// `<skill>/SKILL.md` ← `<skill>/skill-i18n/SKILL.<lang>.md` (ideiglenes fájl + átnevezés, így soha nem félkész).
/// `Ok(false)`, ha nincs forrás (ilyenkor semmi nem változik).
pub fn sync_skill_md(home: &SkillHome, lang: Lang) -> io::Result<bool> {
    let source = match std::fs::read_to_string(home.skill_md_source(lang)) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let tmp = home
        .dir()
        .join(format!(".SKILL.md.tmp-{}", std::process::id()));
    std::fs::write(&tmp, render_skill_md(&source, lang))?;
    if let Err(e) = std::fs::rename(&tmp, home.skill_md_file()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(true)
}

/// Frissítés után: a telepített skill-mappa SKILL.md-je a ténylegesen érvényes nyelven
/// (`CALLGRAPH_LANG` › a mappa `lang` fájlja › angol). Hiba esetén csendes — a frissítés ettől még sikeres.
pub fn sync_installed_skill(dir: &Path) {
    let home = SkillHome::new(
        dir.to_path_buf(),
        dir.display().to_string(),
        dir.join("projects.json"),
    );
    let (lang, _) = i18n::resolve(std::env::var("CALLGRAPH_LANG").ok(), &home);
    let _ = sync_skill_md(&home, lang);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    const EN: &str = include_str!("../../skill/skill-i18n/SKILL.en.md");
    const HU: &str = include_str!("../../skill/skill-i18n/SKILL.hu.md");
    const COMMITTED: &str = include_str!("../../skill/SKILL.md");

    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Self {
            let n = N.fetch_add(1, Ordering::SeqCst);
            let p = std::env::temp_dir().join(format!("callgraph-sync-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(p.join("skill-i18n")).unwrap();
            Self(p)
        }
        fn home(&self) -> SkillHome {
            SkillHome::new(self.0.clone(), "/S".into(), self.0.join("projects.json"))
        }
        fn with_sources(self) -> Self {
            std::fs::write(self.0.join("skill-i18n/SKILL.en.md"), EN).unwrap();
            std::fs::write(self.0.join("skill-i18n/SKILL.hu.md"), HU).unwrap();
            self
        }
        fn skill_md(&self) -> String {
            std::fs::read_to_string(self.0.join("SKILL.md")).unwrap()
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn committed_skill_md_is_the_generated_english_one() {
        assert_eq!(
            COMMITTED,
            render_skill_md(EN, Lang::En),
            "skill/SKILL.md is out of date: regenerate it from skill/skill-i18n/SKILL.en.md"
        );
    }

    #[test]
    fn both_sources_share_name_and_code() {
        for src in [EN, HU] {
            assert!(src.starts_with("---\nname: callgraph\ndescription: "));
        }
        let fences = |s: &str| s.matches("```").count();
        assert_eq!(fences(EN), fences(HU));
        // a parancsok azonosak (a megjegyzés és az idézőjeles helyőrző-szöveg fordítható; a projekt-út helyőrző)
        let commands = |s: &str| {
            s.lines()
                .filter(|l| l.starts_with("bash ") && !l.contains("projects.sh add"))
                .map(|l| l.split(['#', '"']).next().unwrap().trim_end().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(commands(EN), commands(HU));
    }

    #[test]
    fn render_keeps_frontmatter_first() {
        let out = render_skill_md("---\nname: x\n---\n\n# body\n", Lang::Hu);
        assert_eq!(
            out,
            "---\nname: x\n---\n<!-- generated from skill-i18n/SKILL.hu.md by callgraph set-lang / skill-sync; edit the sources -->\n\n# body\n"
        );
        assert!(render_skill_md("# no frontmatter\n", Lang::En).starts_with("<!-- generated"));
        assert_eq!(
            render_skill_md("---\r\nname: x\r\n---\r\n\r\n# body\r\n", Lang::Hu),
            out,
            "CRLF source"
        );
    }

    #[test]
    fn sync_switches_en_hu_en_and_overwrites() {
        let t = Tmp::new().with_sources();
        std::fs::write(t.0.join("SKILL.md"), "old").unwrap();
        assert!(sync_skill_md(&t.home(), Lang::En).unwrap());
        assert_eq!(t.skill_md(), COMMITTED);
        assert!(sync_skill_md(&t.home(), Lang::Hu).unwrap());
        assert_eq!(t.skill_md(), render_skill_md(HU, Lang::Hu));
        assert!(t.skill_md().starts_with("---\nname: callgraph\n"));
        assert!(sync_skill_md(&t.home(), Lang::En).unwrap());
        assert_eq!(t.skill_md(), COMMITTED);
        // nincs ottmaradt ideiglenes fájl
        let leftovers: Vec<_> = std::fs::read_dir(&t.0)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".SKILL"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn missing_source_changes_nothing() {
        let t = Tmp::new();
        std::fs::write(t.0.join("SKILL.md"), "keep").unwrap();
        assert!(!sync_skill_md(&t.home(), Lang::Hu).unwrap());
        assert_eq!(t.skill_md(), "keep");
    }

    #[test]
    fn installed_skill_follows_its_lang_file() {
        if std::env::var_os("CALLGRAPH_LANG").is_some() {
            return; // a környezet felülírná a lang fájlt
        }
        let t = Tmp::new().with_sources();
        std::fs::write(t.0.join("SKILL.md"), COMMITTED).unwrap();
        std::fs::write(t.0.join("lang"), "hu\n").unwrap();
        sync_installed_skill(&t.0);
        assert_eq!(t.skill_md(), render_skill_md(HU, Lang::Hu));
    }
}
