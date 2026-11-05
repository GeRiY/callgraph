use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::{CallgraphError, Result};

use super::{CompactStats, Header, JsonlCacheStore, RowIn, RowOut, Slot};

/// Az aktuális verziójú, utolsó sorok indexe, és a fájl összes (nem üres) sorának száma.
pub(super) fn read_index(store: &JsonlCacheStore) -> Result<(HashMap<String, Slot>, usize)> {
    let mut bytes = Vec::new();
    match File::open(&store.file) {
        Ok(mut file) => {
            file.read_to_end(&mut bytes)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut index = HashMap::new();
    let mut total = 0;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        total += 1;
        let Ok(header) = serde_json::from_str::<Header>(line) else {
            continue;
        };
        if header.version == store.version
            || header
                .version
                .starts_with(&format!("{}+lang=", store.version))
        {
            index.insert(
                header.path,
                Slot {
                    mtime: header.mtime,
                    version: header.version,
                    line: line.to_owned(),
                },
            );
        }
    }
    Ok((index, total))
}

pub(super) fn rewrite(
    store: &mut JsonlCacheStore,
    keep: impl Fn(&str) -> bool,
) -> Result<CompactStats> {
    let total = match fs::read(&store.file) {
        Ok(bytes) => String::from_utf8_lossy(&bytes)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CompactStats::default());
        }
        Err(error) => return Err(error.into()),
    };
    let (mut fresh, _) = store.read_index()?;
    fresh.retain(|path, slot| keep(path) && serde_json::from_str::<RowIn>(&slot.line).is_ok());
    let mut rows: Vec<(&String, &Slot)> = fresh.iter().collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    let mut tmp_name: OsString = store.file.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    let mut out = File::create(&tmp)?;
    for (_, slot) in &rows {
        out.write_all(slot.line.as_bytes())?;
        out.write_all(b"\n")?;
    }
    out.sync_all()?;
    fs::rename(&tmp, &store.file)?;
    let stats = CompactStats {
        kept: fresh.len(),
        dropped: total.saturating_sub(fresh.len()),
    };
    store.index = fresh;
    Ok(stats)
}

impl JsonlCacheStore {
    pub(crate) fn get_for_version(
        &self,
        path: &Path,
        mtime: u64,
        version: &str,
    ) -> Option<crate::model::FileGraph> {
        let slot = self.index.get(path.to_str()?)?;
        if slot.mtime != mtime || slot.version != version {
            return None;
        }
        serde_json::from_str::<RowIn>(&slot.line)
            .ok()
            .map(|row| row.graph)
    }

    pub(crate) fn put_for_version(
        &mut self,
        path: &Path,
        mtime: u64,
        graph: &crate::model::FileGraph,
        version: &str,
    ) -> Result<()> {
        if !path.is_absolute() {
            return Err(CallgraphError::Usage(format!(
                "a cache-kulcs abszolút út kell legyen: {}",
                path.display()
            )));
        }
        let key = path.to_string_lossy().into_owned();
        let mut line = serde_json::to_string(&RowOut {
            path: &key,
            mtime,
            version,
            graph,
        })?;
        line.push('\n');
        let _lock = self.lock(true)?;
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&self.file)?;
        let len = file.metadata()?.len();
        if len > 0 {
            let mut last = [0u8; 1];
            file.seek(SeekFrom::Start(len - 1))?;
            file.read_exact(&mut last)?;
            if last[0] != b'\n' {
                line.insert(0, '\n');
            }
        }
        file.write_all(line.as_bytes())?;
        file.flush()?;
        let stored = line
            .trim_start_matches('\n')
            .trim_end_matches('\n')
            .to_owned();
        self.index.insert(
            key,
            Slot {
                mtime,
                version: version.to_owned(),
                line: stored,
            },
        );
        Ok(())
    }
}
