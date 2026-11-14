//! Command minta: minden fájlt módosító lépés egy parancs-objektum (`WriteNew`, `InsertInto`, `ReplaceRange`,
//! `Undo`), egységes `execute` felülettel. Az új szöveget a `Generator` már elkészítette és ellenőrizte; a parancs
//! csak írja. Minden meglévő fájlt érintő parancs előtt mentés készül (`Backups`, Memento), így az `Undo` is
//! egységesen működik — és maga is visszavonható, mert a visszaállítás előtt a jelenlegi állapotot is menti.

use std::io::ErrorKind;

use super::backup::{Backups, Journal, JournalEntry};
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
        Ok(format!(
            "{} · {}",
            self.summary,
            backup_note(backup.as_deref())
        ))
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
        Ok(format!(
            "{} · {}",
            self.summary,
            backup_note(backup.as_deref())
        ))
    }
}

/// Egy fájl a csoportos íráshoz: a teljes új szöveggel (a fájl lehet új vagy meglévő).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupFile {
    pub rel: String,
    pub text: String,
}

/// Több fájl egy tranzakcióban (preset): mindegyik előtt mentés, az írás után egy napló, amelyből egyetlen `undo`
/// az összes fájlt visszaállítja. Ha egy fájl írása hibázik, az addig írtak visszaállnak.
pub struct WriteGroup {
    pub files: Vec<GroupFile>,
}

impl WriteGroup {
    /// Kiírja a fájlokat; a visszaadott sorok a fájlok sorrendjében: `(fájl, mentés-útja)` (`None`: új fájl).
    pub fn execute_all(&self, env: Env<'_>) -> GenResult<Vec<JournalEntry>> {
        for file in &self.files {
            if env.fs.is_protected(&env.root.join(&file.rel)) {
                return Err(protected(&file.rel));
            }
        }
        let mut done: Vec<JournalEntry> = Vec::new();
        for file in &self.files {
            match write_one(env, file) {
                Ok(entry) => done.push(entry),
                Err(e) => {
                    rollback(env, &done);
                    return Err(e);
                }
            }
        }
        if let Err(e) = Backups::new(env).write_journal(&done) {
            rollback(env, &done);
            return Err(e);
        }
        Ok(done)
    }
}

/// Egy fájl írása: meglévőnél mentéssel, újnál `create_new`-val.
fn write_one(env: Env<'_>, file: &GroupFile) -> GenResult<JournalEntry> {
    let path = env.root.join(&file.rel);
    if env.fs.exists(&path) {
        let backup = commit(env, &file.rel, &file.text)?;
        return Ok(JournalEntry {
            rel: file.rel.clone(),
            backup,
        });
    }
    env.fs.create_new(&path, &file.text).map_err(|e| {
        GenError::new(crate::tf!(
            "cannot write {}: {}",
            "nem írható: {}: {}",
            file.rel,
            e
        ))
    })?;
    Ok(JournalEntry {
        rel: file.rel.clone(),
        backup: None,
    })
}

/// A már kiírt fájlok visszaállítása hiba után (fordított sorrendben; a hibákat nem jelentjük, a mentések
/// megmaradnak).
fn rollback(env: Env<'_>, done: &[JournalEntry]) {
    for entry in done.iter().rev() {
        let path = env.root.join(&entry.rel);
        match &entry.backup {
            Some(backup) => {
                if let Ok(old) = env.fs.read(&env.root.join(backup)) {
                    let _ = env.fs.write(&path, &old);
                }
            }
            None => {
                let _ = env.fs.remove(&path);
            }
        }
    }
}

/// A csoportos írás visszavonása egy lépésben: a mentésből visszaállított fájlok és a törölt új fájlok sorai.
fn undo_group(env: Env<'_>, backups: &Backups<'_>, journal: &Journal) -> GenResult<String> {
    // előbb minden mentés beolvasása: ha bármelyik hiányzik, semmi nem változik
    let mut plan: Vec<(&JournalEntry, Option<String>)> = Vec::new();
    for entry in &journal.entries {
        let content = match &entry.backup {
            Some(backup) => Some(env.fs.read(&env.root.join(backup)).map_err(|e| {
                GenError::new(crate::tf!(
                    "cannot read the backup {}: {}",
                    "a mentés nem olvasható: {}: {}",
                    backup,
                    e
                ))
            })?),
            None => None,
        };
        plan.push((entry, content));
    }
    let mut lines = Vec::new();
    for (entry, content) in plan {
        match (&entry.backup, content) {
            (Some(backup), Some(old)) => {
                commit(env, &entry.rel, &old)?;
                lines.push(crate::tf!(
                    "restored {} from {}",
                    "visszaállítva: {} ← {}",
                    entry.rel,
                    backup
                ));
            }
            _ => {
                let path = env.root.join(&entry.rel);
                if env.fs.is_protected(&path) {
                    return Err(protected(&entry.rel));
                }
                env.fs.remove(&path).map_err(|e| {
                    GenError::new(crate::tf!(
                        "cannot remove {}: {}",
                        "nem törölhető: {}: {}",
                        entry.rel,
                        e
                    ))
                })?;
                lines.push(crate::tf!(
                    "removed {} (created by the last generate)",
                    "törölve: {} (az utolsó generálás hozta létre)",
                    entry.rel
                ));
            }
        }
    }
    backups.clear_journal();
    Ok(lines.join("\n"))
}

/// A fájl legutóbbi mentésének visszaállítása (fájl nélkül: a legutóbb mentetté; ha az utolsó művelet egy preset
/// volt, annak összes fájlja egy lépésben).
pub struct Undo {
    pub rel: Option<String>,
}

impl EditCommand for Undo {
    fn execute(&self, env: Env<'_>) -> GenResult<String> {
        let backups = Backups::new(env);
        if self.rel.is_none()
            && let Some(journal) = backups.current_journal()
        {
            return undo_group(env, &backups, &journal);
        }
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
