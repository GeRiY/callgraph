//! A `fájl[::metódus]` lekérdezési argumentumok értelmezése.

use crate::error::{Result, CallgraphError};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub file: PathBuf,
    pub method: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorSet {
    pub items: Vec<Selector>,
    pub selected: bool,
}

impl SelectorSet {
    pub fn parse(paths: &[PathBuf]) -> Result<Self> {
        let selected = paths.iter().any(|p| p.to_string_lossy().contains("::"));
        if selected && paths.iter().any(|p| !p.to_string_lossy().contains("::")) {
            return Err(CallgraphError::Usage(
                "Ne keverd a :: szelektoros és a sima fájl-argumentumokat egy hívásban.".into(),
            ));
        }
        let items = paths
            .iter()
            .map(|p| {
                let text = p.to_string_lossy();
                text.split_once("::").map_or_else(
                    || {
                        Ok(Selector {
                            file: p.clone(),
                            method: None,
                        })
                    },
                    |(file, method)| {
                        if method.is_empty() {
                            Err(CallgraphError::Usage("üres metódus-szelektor".into()))
                        } else {
                            Ok(Selector {
                                file: PathBuf::from(file),
                                method: Some(method.to_owned()),
                            })
                        }
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { items, selected })
    }
    pub fn methods_for(&self, file: &std::path::Path) -> Option<Vec<&str>> {
        if !self.selected {
            return None;
        }
        Some(
            self.items
                .iter()
                .filter(|s| s.file == file)
                .filter_map(|s| s.method.as_deref())
                .collect(),
        )
    }
}
