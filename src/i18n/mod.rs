//! Kétnyelvű (angol / magyar) terminál-kimenet. A szövegek a hívás helyén élnek, mindkét nyelven:
//! `t!("Usage", "Használat")`, formázva `tf!("{n} files", "{n} fájl")`.
//!
//! A nyelv futásonként egyszer dől el: `CALLGRAPH_LANG` › a skill-mappa `lang` fájlja (`callgraph lang <en|hu>`
//! írja) › alapértelmezés: angol.

use std::sync::OnceLock;

use crate::skill::SkillHome;

/// A támogatott nyelvek.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Hu,
}

impl Lang {
    /// Az összes támogatott nyelv, a súgó és a hibaüzenetek sorrendjében.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Hu];

    /// `en` / `hu` (kis- és nagybetű mindegy, a régió-utótag — `hu_HU.UTF-8` — elhagyható).
    pub fn parse(value: &str) -> Option<Self> {
        let code = value
            .trim()
            .split(['_', '-', '.'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match code.as_str() {
            "en" => Some(Lang::En),
            "hu" => Some(Lang::Hu),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Hu => "hu",
        }
    }

    /// A nyelv neve a saját nyelvén (a `callgraph lang` listájához).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Hu => "Magyar",
        }
    }

    /// A két szöveg közül az ehhez a nyelvhez tartozó.
    pub fn pick<T>(self, en: T, hu: T) -> T {
        match self {
            Lang::En => en,
            Lang::Hu => hu,
        }
    }
}

/// Honnan jött az aktuális nyelv (a `callgraph lang` kiírásához).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangSource {
    Env,
    File,
    Default,
}

static CURRENT: OnceLock<(Lang, LangSource)> = OnceLock::new();

/// Az aktuális nyelv (első híváskor dől el, utána nem változik).
pub fn lang() -> Lang {
    current().0
}

/// Az aktuális nyelv és a forrása.
pub fn current() -> (Lang, LangSource) {
    *CURRENT.get_or_init(|| resolve(std::env::var("CALLGRAPH_LANG").ok(), &SkillHome::from_env()))
}

/// A nyelv eldöntése: környezeti változó › a skill-mappa `lang` fájlja › angol.
pub fn resolve(env: Option<String>, home: &SkillHome) -> (Lang, LangSource) {
    if let Some(lang) = env.as_deref().and_then(Lang::parse) {
        return (lang, LangSource::Env);
    }
    if let Some(lang) = std::fs::read_to_string(home.lang_file())
        .ok()
        .as_deref()
        .and_then(Lang::parse)
    {
        return (lang, LangSource::File);
    }
    (Lang::default(), LangSource::Default)
}

/// A két szöveg közül az aktuális nyelvű (`t!` makró mögött).
pub fn tr<T>(en: T, hu: T) -> T {
    lang().pick(en, hu)
}

/// Statikus szöveg két nyelven: `t!("Usage", "Használat")` → `&'static str`.
#[macro_export]
macro_rules! t {
    ($en:expr, $hu:expr $(,)?) => {
        $crate::i18n::tr($en, $hu)
    };
}

/// Formázott szöveg két nyelven: `tf!("{n} files", "{n} fájl")` → `String`.
/// A két minta ugyanazokat a változókat használhatja (inline vagy pozicionális argumentumként).
#[macro_export]
macro_rules! tf {
    ($en:literal, $hu:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::lang() {
            $crate::i18n::Lang::En => format!($en $(, $arg)*),
            $crate::i18n::Lang::Hu => format!($hu $(, $arg)*),
        }
    };
}

#[cfg(test)]
mod tests;
