//! A module that leans on `use` aliases, groups, globs and an inline module.

use std::collections::{BTreeMap as Map, HashSet};
use std::io::{self, Write as _};

use crate::shapes::{self, Circle as Round, Shape};
use crate::state::*;
use serde_json::{Value, json};

pub type Index = Map<String, usize>;

static GREETING: &str = "hello";

/// Builds an index of words.
pub fn index(words: &[&str]) -> Index {
    let mut seen = HashSet::new();
    let mut map = Index::new();
    for (i, w) in words.iter().enumerate() {
        if seen.insert(*w) {
            map.insert(w.to_string(), i);
        }
    }
    map
}

/// Writes a JSON summary to stdout.
pub fn report(state: &AppState) -> io::Result<()> {
    let circle = Round::new(2.0);
    let value: Value = json!({ "greeting": GREETING, "area": circle.area() });
    let mut out = io::stdout().lock();
    writeln!(out, "{value}")?;
    writeln!(out, "{}", shapes::largest(&[]).is_some())?;
    let _ = state.next_id();
    Ok(())
}

mod internal {
    use super::index;

    /// Re-indexes and counts.
    pub(crate) fn count(words: &[&str]) -> usize {
        index(words).len()
    }
}
