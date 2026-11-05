//! A projekt-indexből készített fordított hívó-lekérdezés.

use std::collections::{HashMap, HashSet};

use regex::RegexBuilder;
use serde::Serialize;

use super::record::{FileStamp, IndexRecord};
use crate::error::{CallgraphError, Result};

/// A `callers` JSONL-kimenetének egy sora.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CallerRow {
    pub file: String,
    pub container: Option<String>,
    pub caller: String,
    pub line: u32,
    pub target: String,
    pub level: usize,
    #[serde(skip)]
    pub(crate) guessed: bool,
}

#[derive(Clone)]
struct Edge {
    row: CallerRow,
    lookup: String,
    caller_key: String,
    guessable: bool,
}

/// A teljes projektből épített, memóriabeli fordított hívó-index.
pub(crate) struct CallerIndex {
    edges: Vec<Edge>,
    reverse: HashMap<String, Vec<Edge>>,
}

impl CallerIndex {
    pub(crate) fn from_graphs(graphs: impl IntoIterator<Item = FileGraphRecord>) -> Self {
        Self::from_records(
            graphs
                .into_iter()
                .map(|record| IndexRecord::new(record.path, FileStamp::default(), record.graph)),
        )
    }
    pub fn from_records(records: impl IntoIterator<Item = IndexRecord>) -> Self {
        let mut edges = Vec::new();
        for record in records {
            let namespace = record.graph.namespace.as_deref().unwrap_or("");
            for call in record.graph.calls {
                let caller = call.caller.clone().unwrap_or_else(|| "__top__".to_string());
                let target = call.resolved.clone().unwrap_or_else(|| call.target.clone());
                let lookup = if call.resolved.is_none() && call.category == "internal_helper" {
                    symbol_key(namespace, call.container.as_deref(), &call.target)
                } else {
                    target.clone()
                };
                let caller_key = symbol_key(namespace, call.container.as_deref(), &caller);
                edges.push(Edge {
                    row: CallerRow {
                        file: record.path.clone(),
                        container: call.container,
                        caller,
                        line: call.line,
                        target,
                        level: 0,
                        guessed: false,
                    },
                    lookup,
                    caller_key,
                    guessable: call.resolved.is_none() && call.category == "external_call",
                });
            }
        }
        let mut reverse: HashMap<String, Vec<Edge>> = HashMap::new();
        for edge in &edges {
            if !edge.guessable || edge.row.target.contains('\\') || edge.row.target.contains("::") {
                reverse
                    .entry(edge.lookup.clone())
                    .or_default()
                    .push(edge.clone());
            }
        }
        Self { edges, reverse }
    }

    pub fn callers(&self, pattern: &str, depth: usize) -> Result<Vec<CallerRow>> {
        self.callers_with_guess(pattern, depth, false)
    }

    pub fn callers_with_guess(
        &self,
        pattern: &str,
        depth: usize,
        guess: bool,
    ) -> Result<Vec<CallerRow>> {
        let matcher = RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map_err(|error| CallgraphError::Usage(format!("hibás minta: {error}")))?;
        let mut result = Vec::new();
        let mut front: Vec<Edge> = self
            .edges
            .iter()
            .filter(|edge| matcher.is_match(&edge.row.target))
            .cloned()
            .collect();
        if guess {
            let names: HashSet<_> = self
                .edges
                .iter()
                .map(|edge| edge.lookup.rsplit("::").next().unwrap_or(&edge.lookup))
                .collect();
            front.extend(
                self.edges
                    .iter()
                    .filter(|edge| {
                        edge.row
                            .target
                            .rsplit("::")
                            .next()
                            .is_some_and(|name| names.contains(name))
                    })
                    .cloned(),
            );
        }
        let mut seen = HashSet::new();
        for level in 1..=depth {
            if front.is_empty() {
                break;
            }
            let mut next = Vec::new();
            for edge in front {
                let key = format!("{}:{}:{}", edge.row.file, edge.caller_key, edge.row.line);
                if !seen.insert(key) {
                    continue;
                }
                let mut row = edge.row.clone();
                row.level = level;
                result.push(row);
                if let Some(parents) = self.reverse.get(&edge.caller_key) {
                    next.extend(parents.iter().cloned());
                }
            }
            front = next;
        }
        result.sort_by(|a, b| {
            a.level
                .cmp(&b.level)
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });
        Ok(result)
    }

    pub fn parents_of_with_skips(
        &self,
        front_rows: &[CallerRow],
        depth: usize,
        guess: bool,
    ) -> (Vec<CallerRow>, Vec<usize>) {
        let mut front: Vec<Edge> = front_rows
            .iter()
            .flat_map(|row| {
                self.edges
                    .iter()
                    .filter(move |edge| {
                        edge.row.file == row.file
                            && edge.row.container == row.container
                            && edge.row.caller == row.caller
                            && edge.row.line == row.line
                    })
                    .cloned()
            })
            .collect();
        let mut result = Vec::new();
        let mut skipped = Vec::new();
        let mut seen = front_rows
            .iter()
            .map(|row| {
                format!(
                    "{}:{}:{}",
                    row.file,
                    row.container.as_deref().unwrap_or("__top__"),
                    row.caller
                )
            })
            .collect::<HashSet<_>>();
        let initial_seen = seen.clone();
        for level in 2..=depth {
            let mut next = Vec::new();
            let mut guessed = Vec::new();
            for edge in front {
                if let Some(parents) = self.reverse.get(&edge.caller_key) {
                    next.extend(parents.iter().cloned());
                }
                let name = edge
                    .row
                    .caller
                    .rsplit("::")
                    .next()
                    .unwrap_or(&edge.row.caller);
                guessed.extend(
                    self.edges
                        .iter()
                        .filter(|parent| {
                            parent.guessable
                                && parent.row.target == name
                                && parent.row.target == parent.lookup
                                && parent.row.target != parent.row.caller
                        })
                        .cloned(),
                );
            }
            guessed.sort_by(|left, right| {
                left.row
                    .file
                    .cmp(&right.row.file)
                    .then_with(|| left.row.line.cmp(&right.row.line))
            });
            guessed.dedup_by(|left, right| {
                left.row.file == right.row.file
                    && left.row.container == right.row.container
                    && left.row.caller == right.row.caller
            });
            let guessed_keys = guessed
                .iter()
                .map(|edge| {
                    format!(
                        "{}:{}:{}",
                        edge.row.file,
                        edge.row.container.as_deref().unwrap_or("__top__"),
                        edge.row.caller
                    )
                })
                .collect::<HashSet<_>>();
            if guess {
                for edge in guessed {
                    let mut edge = edge;
                    edge.row.guessed = true;
                    next.push(edge);
                }
            } else {
                skipped.push(guessed_keys.len());
                next.retain(|edge| {
                    !edge.guessable
                        || edge.row.target.contains('\\')
                        || edge.row.target.contains("::")
                });
            }
            // Hívónként a legkisebb sorú hívás marad (a régi: fájlon belül hívás-sorrend).
            next.sort_by(|a, b| (&a.row.file, a.row.line).cmp(&(&b.row.file, b.row.line)));
            front = next
                .into_iter()
                .filter(|edge| {
                    let key = format!(
                        "{}:{}:{}",
                        edge.row.file,
                        edge.row.container.as_deref().unwrap_or("__top__"),
                        edge.row.caller
                    );
                    (edge.guessable && !initial_seen.contains(&key)) || seen.insert(key)
                })
                .map(|mut edge| {
                    edge.row.guessed = edge.guessable;
                    edge
                })
                .collect();
            for edge in &front {
                let mut row = edge.row.clone();
                row.level = level;
                result.push(row);
            }
        }
        (result, skipped)
    }
}

pub(crate) struct FileGraphRecord {
    pub path: String,
    pub graph: crate::model::FileGraph,
}

fn symbol_key(namespace: &str, container: Option<&str>, caller: &str) -> String {
    let prefix = if namespace.is_empty() {
        String::new()
    } else {
        format!("{namespace}\\")
    };
    match container {
        Some(container) => format!("{prefix}{container}::{caller}"),
        None => format!("{prefix}{caller}"),
    }
}
