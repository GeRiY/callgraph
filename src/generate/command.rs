//! Command minta: minden fájlt módosító lépés egy parancs-objektum (`WriteNew`, `InsertInto`, `ReplaceRange`,
//! `Undo`), egységes `execute` felülettel. Az új szöveget a `Generator` már elkészítette és ellenőrizte; a parancs
//! csak írja. Minden meglévő fájlt érintő parancs előtt mentés készül (`Backups`, Memento), így az `Undo` is
//! egységesen működik — és maga is visszavonható, mert a visszaállítás előtt a jelenlegi állapotot is menti.

use std::io::ErrorKind;

use super::backup::Backups;
use super::error::{GenError, GenResult};
use super::fs::Env;

/// Egy végrehajtható szerkesztés.
pub trait EditCommand {
    /// Elvégzi az írást; az egysoros jelentést adja vissza (mi hova került, melyik mentés készült).
    fn execute(&self, env: Env<'_>) -> GenResult<String>;
}

fn backup_note(backup: Option<&str>) -> String {
    match backup {
        Some(path) => crate::tf!("backup: {}", "mentés: {}", path),
        None => crate::t!("backup: none", "mentés: nincs").to_owned(),
    }
}

fn protected(rel: &str) -> GenError {
    GenError::new(crate::tf!(
        "{} is protected by .callgraphignore (`protect`), not modifying it",
        "{} védett a .callgraphignore-ban (`protect`), nem módosítom",
        rel
    ))
}

/// Meglévő (vagy új) fájl írása: előbb a mentés, utána az írás. A mentés útját adja vissza.
fn commit(env: Env<'_>, rel: &str, text: &str) -> GenResult<Option<String>> {
    let path = env.root.join(rel);
    if env.fs.is_protected(&path) {
        return Err(protected(rel));
    }
    let backup = if env.fs.exists(&path) {
        let old = env.fs.read(&path).map_err(|e| {
            GenError::new(crate::tf!(
                "cannot read {}: {}",
                "nem olvasható: {}: {}",
                rel,
                e
            ))
        })?;
        Some(Backups::new(env).save(rel, &old)?)
    } else {
        None
    };
    env.fs.write(&path, text).map_err(|e| {
        GenError::new(crate::tf!(
            "cannot write {}: {}",
            "nem írható: {}: {}",
            rel,
            e
        ))
    })?;
    Ok(backup)
}

/// Új fájl létrehozása; létezőt soha nem ír felül.
pub struct WriteNew {
    pub rel: String,
    pub text: String,
}

impl EditCommand for WriteNew {
    fn execute(&self, env: Env<'_>) -> GenResult<String> {
        let path = env.root.join(&self.rel);
        if env.fs.is_protected(&path) {
            return Err(protected(&self.rel));
        }
        env.fs.create_new(&path, &self.text).map_err(|e| {
            if e.kind() == ErrorKind::AlreadyExists {
                GenError::new(crate::tf!(
                    "{} already exists — to change it use --into <file>::Class or --replace <file>::method",
                    "{} már létezik — módosításhoz használd: --into <fájl>::Osztály vagy --replace <fájl>::metódus",
                    self.rel
                ))
            } else {
                GenError::new(crate::tf!(
                    "cannot write {}: {}",
                    "nem írható: {}: {}",
                    self.rel,
                    e
                ))
            }
        })?;
        let lines = self.text.lines().count();
        Ok(crate::tf!(
            "wrote {} ({} lines) · {}",
            "írtam: {} ({} sor) · {}",
            self.rel,
            lines,
            backup_note(None)
        ))
    }
}

/// Tagok beszúrása egy meglévő osztályba (a teljes új szöveggel, a leíró jelentés-előtaggal).
pub struct InsertInto {
    pub rel: String,
    pub text: String,
    pub summary: String,
}

impl EditCommand for InsertInto {
    fn execute(&self, env: Env<'_>) -> GenResult<String> {
        let backup = commit(env, &self.rel, &self.text)?;
        Ok(format!("{} · {}", self.summary, backup_note(backup.as_deref())))
    }
}

/// Sortartomány / metódus cseréje (a teljes új szöveggel, a leíró jelentés-előtaggal).
pub struct ReplaceRange {
    pub rel: String,
    pub text: String,
    pub summary: String,
}

impl EditCommand for ReplaceRange {
    fn execute(&self, env: Env<'_>) -> GenResult<String> {
        let backup = commit(env, &self.rel, &self.text)?;
        Ok(format!("{} · {}", self.summary, backup_note(backup.as_deref())))
    }
}

/// A fájl legutóbbi mentésének visszaállítása (fájl nélkül: a legutóbb mentetté).
pub struct Undo {
    pub rel: Option<String>,
}

impl EditCommand for Undo {
    fn execute(&self, env: Env<'_>) -> GenResult<String> {
        let backups = Backups::new(env);
        let found = match &self.rel {
            Some(rel) => backups.newest(rel).map(|path| (rel.clone(), path)),
            None => backups.newest_overall(),
        };
        let Some((rel, source)) = found else {
            return Err(GenError::new(match &self.rel {
                Some(rel) => crate::tf!(
                    "no backup of {} (backups: .callgraph/backups/)",
                    "nincs mentés ehhez: {} (a mentések: .callgraph/backups/)",
                    rel
                ),
                None => crate::t!(
                    "no backups yet (backups: .callgraph/backups/)",
                    "még nincs mentés (a mentések: .callgraph/backups/)"
                )
                .to_owned(),
            }));
        };
        let content = env.fs.read(&source).map_err(|e| {
            GenError::new(crate::tf!(
                "cannot read the backup {}: {}",
                "a mentés nem olvasható: {}: {}",
                source.display(),
                e
            ))
        })?;
        let backup = commit(env, &rel, &content)?;
        let from = super::project::relative(env.root, &source)
            .unwrap_or_else(|| source.display().to_string());
        Ok(crate::tf!(
            "restored {} from {} · {}",
            "visszaállítva: {} ← {} · {}",
            rel,
            from,
            backup_note(backup.as_deref())
        ))
    }
}
