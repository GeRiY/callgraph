use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;

use crate::error::Result;

use super::JsonlCacheStore;

impl JsonlCacheStore {
    fn lock_path(&self) -> PathBuf {
        let mut name: OsString = self.file.as_os_str().to_owned();
        name.push(".lock");
        PathBuf::from(name)
    }

    /// Oldalfájl-zár; a fájl eldobásakor a zár felszabadul.
    pub(crate) fn lock(&self, exclusive: bool) -> Result<File> {
        if let Some(dir) = self.file.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.lock_path())?;
        if exclusive {
            lock.lock()?;
        } else {
            lock.lock_shared()?;
        }
        Ok(lock)
    }
}
