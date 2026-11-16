//! Bájt-pontos szerkesztések és a közös végrehajtás: a tervezők (`route`, `class`, `method`) `Site`-okat gyűjtenek
//! (fájl, bájt-tartomány, új szöveg), a `finish` ezekből fájlonként előállítja az új szöveget, ellenőrzi (PHP: mago,
//! Vue / JS: oxc), és egy tranzakcióban írja (`WriteGroup`: mentés + napló, egyetlen `undo`). Bármely hiba → semmi
//! nem íródik.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::generate::command::{GroupFile, WriteGroup};
use crate::generate::fs::{Env, FileSystem};
use crate::generate::project::relative;
use crate::generate::{Emitter, GenError, GenResult, Output, PhpEmitter, vue};

/// A kimenetben legfeljebb ennyi `⚠ bizonytalan` sor jelenik meg (a többi `+N`).
const MAX_UNCERTAIN: usize = 10;
/// `--dry`: legfeljebb ennyi hely az előnézetben.
const MAX_PREVIEW: usize = 50;
/// A fájl-összesítő sorok szélessége.
const WRAP: usize = 100;

/// Egy szerkesztés: a `[start, end)` bájt-tartomány cseréje.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Site {
    pub rel: String,
    pub start: usize,
    pub end: usize,
    pub new: String,
}

impl Site {
    pub(crate) fn replace(rel: &str, start: usize, end: usize, new: impl Into<String>) -> Self {
        Self {
            rel: rel.to_owned(),
            start,
            end,
            new: new.into(),
        }
    }

    pub(crate) fn insert(rel: &str, at: usize, new: impl Into<String>) -> Self {
        Self::replace(rel, at, at, new)
    }
}

/// Egy hely, amit nem változtattunk meg, mert nem biztos, hogy ugyanarra vonatkozik.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Uncertain {
    pub rel: String,
    pub line: usize,
    pub text: String,
}

/// Egy átnevezés terve.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    pub sites: Vec<Site>,
    pub uncertain: Vec<Uncertain>,
    /// Fájl-áthelyezések `(régi, új)`; a régi út szerkesztései az új helyre íródnak.
    pub moves: Vec<(String, String)>,
    /// Figyelmeztetések (`⚠ …`), a kimenet végén.
    pub notes: Vec<String>,
}

/// Mappák, amelyekbe a bejárás nem lép be.
fn skipped_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "vendor" | "node_modules" | "storage" | "cache" | "build" | "dist"
        )
}

/// A projekt fájljai a megadott kiterjesztésekkel (mélységi bejárás, rendezve).
pub(crate) fn walk(fs: &dyn FileSystem, root: &Path, exts: &[&str]) -> Vec<(String, PathBuf)> {
    let mut stack = vec![root.to_path_buf()];
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(mut entries) = fs.list(&dir) else {
            continue;
        };
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        for entry in entries {
            let name = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_owned();
            if entry.is_dir {
                if !skipped_dir(&name) {
                    stack.push(entry.path);
                }
            } else if exts.iter().any(|e| name.ends_with(&format!(".{e}")))
                && let Some(rel) = relative(root, &entry.path)
            {
                out.push((rel, entry.path));
            }
        }
    }
    out.sort();
    out
}

/// A szöveg 1-alapú sora a bájt-helyen.
pub(crate) fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()
        .iter()
        .take(offset)
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// A sor szövege a bájt-helyen, levágva (a `⚠` sorokhoz).
pub(crate) fn line_snippet(text: &str, offset: usize) -> String {
    let start = text[..offset].rfind('\n').map_or(0, |p| p + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |p| offset + p);
    text[start..end].trim().chars().take(80).collect()
}

/// Egy kész szöveg ellenőrzése a fájl fajtája szerint (Blade és ismeretlen kiterjesztés: nincs ellenőrzés).
fn check_text(rel: &str, text: &str) -> Result<(), String> {
    if rel.ends_with(".blade.php") {
        return Ok(());
    }
    if rel.ends_with(".php") {
        return PhpEmitter
            .check(rel, text)
            .map_err(|(line, message)| format!("L{line}: {message}"));
    }
    if rel.ends_with(".vue") {
        return vue::check_sfc(text);
    }
    let script = [".js", ".ts", ".jsx", ".tsx", ".mjs"];
    if script.iter().any(|e| rel.ends_with(e)) {
        return vue::check_script(rel, text);
    }
    Ok(())
}

/// A szerkesztések alkalmazása egy fájl szövegén; `Err`: átfedő vagy érvénytelen tartomány.
fn apply(rel: &str, old: &str, sites: &[Site]) -> GenResult<String> {
    let mut out = String::with_capacity(old.len() + 64);
    let mut at = 0;
    for site in sites {
        if site.start < at
            || site.end > old.len()
            || site.start > site.end
            || !old.is_char_boundary(site.start)
            || !old.is_char_boundary(site.end)
        {
            return Err(GenError::new(crate::tf!(
                "conflicting edits in {} at byte {} (nothing written)",
                "ütköző szerkesztések itt: {} a(z) {}. bájtnál (nem írtam semmit)",
                rel,
                site.start
            )));
        }
        out.push_str(&old[at..site.start]);
        out.push_str(&site.new);
        at = site.end;
    }
    out.push_str(&old[at..]);
    Ok(out)
}

fn read(env: Env<'_>, rel: &str) -> GenResult<String> {
    env.fs.read(&env.root.join(rel)).map_err(|e| {
        GenError::new(crate::tf!(
            "cannot read {}: {}",
            "nem olvasható: {}: {}",
            rel,
            e
        ))
    })
}

/// Az egy soros jelentés sorai: a fájl-összesítők `·`-tal, ~100 karakteres sorokba tördelve.
fn wrap(parts: &[String]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for part in parts {
        match lines.last_mut() {
            Some(last) if last.chars().count() + part.chars().count() + 3 <= WRAP => {
                last.push_str(" · ");
                last.push_str(part);
            }
            _ => lines.push(part.clone()),
        }
    }
    lines
}

/// A terv végrehajtása (vagy `--dry`: csak a jelentés). `what`: pl. `route profile.edit → account.edit`.
pub(crate) fn finish(env: Env<'_>, what: &str, plan: Plan, dry: bool) -> GenResult<Output> {
    let Plan {
        sites,
        uncertain,
        moves,
        notes,
    } = plan;
    if sites.is_empty() {
        return Err(GenError::new(crate::tf!(
            "nothing to rename: {} has no occurrences",
            "nincs mit átnevezni: {} sehol nem fordul elő",
            what
        )));
    }
    let mut by_file: BTreeMap<String, Vec<Site>> = BTreeMap::new();
    for site in sites {
        by_file.entry(site.rel.clone()).or_default().push(site);
    }
    let target_of = |rel: &str| -> String {
        moves
            .iter()
            .find(|(from, _)| from == rel)
            .map_or_else(|| rel.to_owned(), |(_, to)| to.clone())
    };
    for (from, to) in &moves {
        if env.fs.exists(&env.root.join(to)) {
            return Err(GenError::new(crate::tf!(
                "{} already exists, not moving {} onto it (nothing written)",
                "{} már létezik, nem mozgatom rá: {} (nem írtam semmit)",
                to,
                from
            )));
        }
    }

    let mut files: Vec<GroupFile> = Vec::new();
    let mut counts: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut previews: Vec<String> = Vec::new();
    for (rel, mut list) in by_file {
        let old = read(env, &rel)?;
        list.sort_by(|a, b| (a.start, a.end, &a.new).cmp(&(b.start, b.end, &b.new)));
        list.dedup();
        let new = apply(&rel, &old, &list)?;
        let dest = target_of(&rel);
        // csak akkor ellenőrzünk, ha a régi szöveg hibátlan volt: a már eleve hibás fájl nem a mi hibánk
        if check_text(&rel, &old).is_ok()
            && let Err(message) = check_text(&dest, &new)
        {
            return Err(GenError::new(crate::tf!(
                "the edit would break {}: {} (nothing written)",
                "az átnevezés elrontaná ezt: {}: {} (nem írtam semmit)",
                dest,
                message
            )));
        }
        for site in &list {
            previews.push(format!(
                "{}:L{}  {} → {}",
                dest,
                line_of(&old, site.start),
                shorten(&old[site.start..site.end]),
                shorten(&site.new)
            ));
        }
        total += list.len();
        counts.push(if dest == rel {
            format!("{}: {}", dest, list.len())
        } else {
            format!("{rel} → {dest}: {}", list.len())
        });
        files.push(GroupFile {
            rel: dest,
            text: new,
        });
    }
    let file_count = files.len();

    if !dry {
        let removals: Vec<String> = moves.iter().map(|(from, _)| from.clone()).collect();
        WriteGroup { files }.execute_with_removals(env, &removals)?;
    }

    let head = if dry {
        crate::tf!(
            "dry run, would rename {}: {} files, {} sites",
            "próbafutás, átnevezné: {}: {} fájl, {} hely",
            what,
            file_count,
            total
        )
    } else {
        crate::tf!(
            "renamed {}: {} files, {} sites",
            "átnevezve: {}: {} fájl, {} hely",
            what,
            file_count,
            total
        )
    };
    let mut lines = vec![head];
    lines.extend(wrap(&counts));
    if dry {
        let extra = previews.len().saturating_sub(MAX_PREVIEW);
        lines.extend(previews.into_iter().take(MAX_PREVIEW));
        if extra > 0 {
            lines.push(format!("+{extra}"));
        }
    }
    let extra = uncertain.len().saturating_sub(MAX_UNCERTAIN);
    for u in uncertain.iter().take(MAX_UNCERTAIN) {
        lines.push(crate::tf!(
            "⚠ uncertain (not changed): {} L{} {}",
            "⚠ bizonytalan (nem változtattam): {} L{} {}",
            u.rel,
            u.line,
            u.text
        ));
    }
    if extra > 0 {
        lines.push(format!("+{extra}"));
    }
    lines.extend(notes);
    if !dry {
        lines.push(crate::t!("undo: callgraph undo", "visszavonás: callgraph undo").to_owned());
    }
    Ok(Output {
        lines,
        unresolved: Vec::new(),
    })
}

/// Egy előnézet-érték: egy sorra vágva, rövidítve.
fn shorten(text: &str) -> String {
    let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= 60 {
        one
    } else {
        let cut: String = one.chars().take(59).collect();
        format!("{cut}…")
    }
}
