//! A `fájl[::metódus]` lekérdezési argumentumok értelmezése.

use crate::error::{CallgraphError, Result};
use crate::t;
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
                t!(
                    "Don't mix :: selector and plain file arguments in one call.",
                    "Ne keverd a :: szelektoros és a sima fájl-argumentumokat egy hívásban."
                )
                .into(),
            ));
        }
        let mut items: Vec<Selector> = Vec::with_capacity(paths.len());
        for p in paths {
            let text = p.to_string_lossy();
            let Some((file, method)) = text.split_once("::") else {
                items.push(Selector {
                    file: p.clone(),
                    method: None,
                });
                continue;
            };
            if method.is_empty() {
                return Err(CallgraphError::Usage(
                    t!("empty method selector", "üres metódus-szelektor").into(),
                ));
            }
            // a rövid `::m2` az előző szelektor fájlját örökli (`a.php::m1 ::m2`)
            let file = if file.is_empty() {
                let Some(previous) = items.last() else {
                    return Err(CallgraphError::Usage(
                        t!(
                            "::method needs a file before it: <file>::m1 ::m2",
                            "a ::metódus elé fájl kell: <fájl>::m1 ::m2"
                        )
                        .into(),
                    ));
                };
                previous.file.clone()
            } else {
                PathBuf::from(file)
            };
            items.push(Selector {
                file,
                method: Some(method.to_owned()),
            });
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<SelectorSet> {
        SelectorSet::parse(&args.iter().map(PathBuf::from).collect::<Vec<_>>())
    }

    #[test]
    fn a_short_method_selector_inherits_the_previous_file() {
        let set = parse(&["src/A.php::alpha", "::beta", "src/B.php::gamma", "::delta"]).unwrap();
        let pairs: Vec<(String, String)> = set
            .items
            .iter()
            .map(|s| (s.file.display().to_string(), s.method.clone().unwrap()))
            .collect();
        let pair = |f: &str, m: &str| (f.to_owned(), m.to_owned());
        assert_eq!(
            pairs,
            vec![
                pair("src/A.php", "alpha"),
                pair("src/A.php", "beta"),
                pair("src/B.php", "gamma"),
                pair("src/B.php", "delta"),
            ]
        );
        assert_eq!(
            set.methods_for(std::path::Path::new("src/A.php")),
            Some(vec!["alpha", "beta"])
        );
    }

    #[test]
    fn a_short_method_selector_without_a_file_before_it_is_a_usage_error() {
        let error = parse(&["::beta"]).unwrap_err().to_string();
        assert!(
            error.contains("<file>::m1 ::m2") || error.contains("<fájl>::m1 ::m2"),
            "{error}"
        );
        assert!(parse(&["src/A.php::"]).is_err());
    }
}
