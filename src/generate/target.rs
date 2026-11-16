//! A három Facade (`Generator`, `VueGenerator`, `PresetGenerator`) közös, a célfájlt érintő infrastruktúrája: a
//! felhasználói út projekt-relatívvá alakítása (`rel`), a meglévő fájl beolvasása (`load`), az `--expect` sha256-ellenőrzés
//! (`check_expect`) és a kész szöveg szintaxis-ellenőrzése a fájl kiterjesztése szerint (`Checker`).

use std::path::{Component, Path};

use super::edit::{range_text, split_lines};
use super::emitter::Emitter;
use super::error::{GenError, GenResult, Kind};
use super::fs::Env;
use super::php::PhpEmitter;
use super::vue::{Problem, check_html, check_script_file, check_vue, is_html_file};

/// A felhasználói fájl-út projekt-relatív, `/`-es alakja; a gyökéren kívüli út hiba.
pub fn rel(root: &Path, file: &str) -> GenResult<String> {
    let outside = || {
        GenError::new(crate::tf!(
            "{} is outside the project",
            "{} a projekten kívül van",
            file
        ))
    };
    let path = Path::new(file);
    // a gyökér levágása az abszolútság vizsgálata előtt (Windows-on a `/p/x` nem abszolút, de a gyökér alatt van)
    let relative = match path.strip_prefix(root) {
        Ok(rel) => rel,
        Err(_) if path.is_absolute() => return Err(outside()),
        Err(_) => path,
    };
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => return Err(outside()),
        }
    }
    if parts.is_empty() {
        return Err(outside());
    }
    Ok(parts.join("/"))
}

/// Egy beolvasott fájl: a szöveg, a sorai és a sorvég-jel.
pub struct Loaded {
    pub text: String,
    pub lines: Vec<String>,
    pub eol: &'static str,
}

/// A meglévő fájl beolvasása (`rel`: projekt-relatív út); hiányzó vagy nem olvasható fájl egysoros hiba.
pub fn load(env: Env<'_>, rel: &str) -> GenResult<Loaded> {
    let path = env.root.join(rel);
    if !env.fs.exists(&path) {
        return Err(GenError::new(crate::tf!(
            "{} not found (a new file: --write {})",
            "{} nem található (új fájl: --write {})",
            rel,
            rel
        ))
        .with_kind(Kind::NotFound));
    }
    let text = env.fs.read(&path).map_err(|e| {
        GenError::new(crate::tf!(
            "cannot read {}: {}",
            "nem olvasható: {}: {}",
            rel,
            e
        ))
    })?;
    let (lines, eol) = split_lines(&text);
    Ok(Loaded { text, lines, eol })
}

/// Az `--expect`: az 1-alapú, zárt `from..=to` sorok sha256-előtagja egyezik-e (különben valaki közben módosította).
pub fn check_expect(
    lines: &[String],
    rel: &str,
    from: usize,
    to: usize,
    expect: Option<&str>,
) -> GenResult<()> {
    let Some(prefix) = expect else {
        return Ok(());
    };
    let current = crate::update::checksum::sha256_hex(range_text(lines, from, to).as_bytes());
    let wanted = prefix.trim().to_ascii_lowercase();
    if wanted.len() < 4 || !current.starts_with(&wanted) {
        return Err(GenError::new(crate::tf!(
            "L{}-{} of {} changed (expected sha256 {}…, current {}…): read it again before replacing",
            "L{}-{} tartalma megváltozott ebben: {} (várt sha256 {}…, jelenlegi {}…): cserélés előtt olvasd újra",
            from,
            to,
            rel,
            wanted,
            &current[..12]
        ))
        .with_kind(Kind::Changed));
    }
    Ok(())
}

/// A kész szöveg szintaxis-ellenőrzője, a fájl kiterjesztése szerint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checker {
    /// `.php`: mago.
    Php(String),
    /// `.vue`: script (oxc), template (tag-egyensúly), style (CSS-parser).
    Vue,
    /// `.html` / `.htm`: tag-egyensúly, inline style / script.
    Html,
    /// `.ts` / `.js` (és `.mts` / `.cts` / `.mjs` / `.cjs`): oxc.
    Script(String),
    /// Ismeretlen kiterjesztés: nincs ellenőrzés.
    Skip,
}

impl Checker {
    pub fn for_path(rel: &str) -> Self {
        if is_html_file(rel) {
            return Self::Html;
        }
        match rel.rsplit('.').next().unwrap_or("") {
            "php" => Self::Php(rel.to_owned()),
            "vue" => Self::Vue,
            "ts" | "mts" | "cts" | "js" | "mjs" | "cjs" => Self::Script(rel.to_owned()),
            _ => Self::Skip,
        }
    }

    /// Az első hiba (szakasz, 1-alapú sor, üzenet); a `section` PHP-nál és szkriptnél a fájl neve.
    pub fn check(&self, text: &str) -> Result<(), Problem> {
        match self {
            Self::Php(rel) => PhpEmitter
                .check(rel, text)
                .map_err(|(line, message)| Problem {
                    section: rel.clone(),
                    line: line as usize,
                    message,
                }),
            Self::Vue => check_vue(text),
            Self::Html => check_html(text),
            Self::Script(rel) => check_script_file(rel, text),
            Self::Skip => Ok(()),
        }
    }
}
