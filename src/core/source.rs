//! `SourceFile`: egy beolvasott forrásfájl — a nyelvek bemenete.

use std::path::{Path, PathBuf};

use crate::error::{CallgraphError, Result};

/// Egy forrásfájl útja, nyers tartalma és módosítási ideje (másodperc).
///
/// A tartalom bájt-sorozat: a forrás nem feltétlenül érvényes UTF-8, a parser bájtokat kap.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub text: Vec<u8>,
    pub mtime: Option<u64>,
}

impl SourceFile {
    pub fn new(path: impl Into<PathBuf>, text: Vec<u8>, mtime: Option<u64>) -> Self {
        Self {
            path: path.into(),
            text,
            mtime,
        }
    }

    /// Fájl beolvasása a lemezről.
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read(path).map_err(|source| CallgraphError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mtime = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        Ok(Self::new(path, text, mtime))
    }

    /// Az út szövegesen (lossy), a gráf `file` mezőjének alakjában.
    pub fn path_string(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_string_is_lossy_display() {
        let f = SourceFile::new("a/b.php", b"<?php".to_vec(), None);
        assert_eq!(f.path_string(), "a/b.php");
    }

    #[test]
    fn missing_file_is_read_error() {
        assert!(SourceFile::read(Path::new("/nincs/ilyen.php")).is_err());
    }
}
