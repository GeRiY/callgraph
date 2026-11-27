//! Changed lines → the innermost symbols of the file (method / function, else class, else file-level), from the
//! line ranges the project graph already holds.

use std::collections::BTreeMap;

use crate::model::FileGraph;

/// Largest line range of one hunk that is walked line by line (a corrupt diff must not spin).
const MAX_RANGE: u32 = 400_000;

/// What kind of symbol was touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Method,
    Class,
    /// Code outside any method or class (module level, top-level script, imports).
    File,
}

/// A changed symbol with the changed lines inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Touched {
    pub kind: Kind,
    pub container: Option<String>,
    pub name: String,
    pub start: u32,
    pub end: u32,
    /// Changed new-side line ranges inside the symbol.
    pub changed: Vec<(u32, u32)>,
    /// New-side lines after which lines were only removed inside the symbol.
    pub removed_after: Vec<u32>,
}

impl Touched {
    /// `Type::name()`, `name()`, `class Name` or `file-level code`.
    pub(crate) fn label(&self) -> String {
        match self.kind {
            Kind::Method => match &self.container {
                Some(container) => format!("{container}::{}()", self.name),
                None => format!("{}()", self.name),
            },
            Kind::Class => format!("class {}", self.name),
            Kind::File => crate::t!("file-level code", "fájl-szintű kód").to_owned(),
        }
    }

    /// `L20-23` (the whole symbol), empty for file-level code.
    pub(crate) fn span(&self) -> String {
        match (self.kind, self.start == self.end) {
            (Kind::File, _) => String::new(),
            (_, true) => format!("L{}", self.start),
            _ => format!("L{}-{}", self.start, self.end),
        }
    }

    /// `changed L96`, `changed L96-98, L120`, `lines removed near L96`; at most three ranges.
    pub(crate) fn changed_text(&self) -> String {
        let mut parts: Vec<String> = self
            .changed
            .iter()
            .map(|&(a, b)| {
                if a == b {
                    format!("L{a}")
                } else {
                    format!("L{a}-{b}")
                }
            })
            .collect();
        if parts.len() > 3 {
            let rest = parts.len() - 3;
            parts.truncate(3);
            parts.push(format!("+{rest}"));
        }
        let mut text = if parts.is_empty() {
            String::new()
        } else {
            crate::tf!("changed {}", "módosult {}", parts.join(", "))
        };
        if let Some(first) = self.removed_after.first() {
            if !text.is_empty() {
                text.push_str(", ");
            }
            text.push_str(&crate::tf!(
                "lines removed near L{first}",
                "sorok törölve itt: L{first}"
            ));
        }
        text
    }
}

/// A symbol of the file with its line range.
struct Span {
    kind: Kind,
    container: Option<String>,
    name: String,
    start: u32,
    end: u32,
}

impl Span {
    fn holds(&self, lines: &[u32]) -> bool {
        lines
            .iter()
            .all(|&line| self.start <= line && line <= self.end)
    }

    /// Smaller range first; at equal size a method before a class.
    fn rank(&self) -> (u32, u8) {
        (
            self.end.saturating_sub(self.start),
            u8::from(self.kind == Kind::Class),
        )
    }
}

fn spans_of(graph: &FileGraph) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for method in &graph.methods {
        // the pseudo-methods hold the module-level code: that is file-level, not a symbol
        if method.name == "__top__" || method.name == "__module__" {
            continue;
        }
        spans.push(Span {
            kind: Kind::Method,
            container: method.container.clone(),
            name: method.name.clone(),
            start: method.line,
            end: method.end_line.unwrap_or(method.line).max(method.line),
        });
    }
    for container in &graph.containers {
        // a module / file container (Rust `mod`, Python module, file-level namespace) is not a type: the lines
        // in it that are outside every function are file-level code, searching callers by its name is noise
        if container.kind == "module" {
            continue;
        }
        spans.push(Span {
            kind: Kind::Class,
            container: None,
            name: container.name.clone(),
            start: container.line,
            end: container.end_line.max(container.line),
        });
    }
    spans
}

/// The innermost span that holds all `lines`.
fn innermost(spans: &[Span], lines: &[u32]) -> Option<usize> {
    spans
        .iter()
        .enumerate()
        .filter(|(_, span)| span.holds(lines))
        .min_by_key(|(_, span)| span.rank())
        .map(|(at, _)| at)
}

/// The `Touched` of a span (created on first use); `None` = the file-level one.
fn entry<'a>(
    found: &'a mut BTreeMap<usize, Touched>,
    spans: &[Span],
    at: Option<usize>,
) -> &'a mut Touched {
    found
        .entry(at.unwrap_or(usize::MAX))
        .or_insert_with(|| match at {
            Some(index) => Touched {
                kind: spans[index].kind,
                container: spans[index].container.clone(),
                name: spans[index].name.clone(),
                start: spans[index].start,
                end: spans[index].end,
                changed: Vec::new(),
                removed_after: Vec::new(),
            },
            None => Touched {
                kind: Kind::File,
                container: None,
                name: String::new(),
                start: 0,
                end: 0,
                changed: Vec::new(),
                removed_after: Vec::new(),
            },
        })
}

/// The symbols that contain changed lines (`added`: inclusive new-side ranges; `removed_after`: lines removed after
/// that new-side line, mapped to the symbol around the gap), in file order.
pub(crate) fn touched(
    graph: &FileGraph,
    added: &[(u32, u32)],
    removed_after: &[u32],
) -> Vec<Touched> {
    let spans = spans_of(graph);
    // keyed by the span index; `usize::MAX` = file-level
    let mut found: BTreeMap<usize, Touched> = BTreeMap::new();
    for &(from, to) in added {
        let to = to.min(from.saturating_add(MAX_RANGE));
        for line in from..=to {
            let symbol = entry(&mut found, &spans, innermost(&spans, &[line]));
            match symbol.changed.last_mut() {
                Some(last) if last.1 + 1 == line => last.1 = line,
                _ => symbol.changed.push((line, line)),
            }
        }
    }
    for &after in removed_after {
        // the gap lies between `after` and `after + 1`: the symbol must hold both
        let symbol = entry(&mut found, &spans, innermost(&spans, &[after, after + 1]));
        symbol.removed_after.push(after);
    }
    let mut out: Vec<Touched> = found.into_values().collect();
    out.sort_by(|a, b| {
        (a.kind == Kind::File, a.start, &a.name).cmp(&(b.kind == Kind::File, b.start, &b.name))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Container, Method};

    fn method(container: &str, name: &str, line: u32, end: u32) -> Method {
        Method {
            container: Some(container.to_owned()),
            name: name.to_owned(),
            line,
            end_line: Some(end),
            visibility: None,
            is_static: false,
            is_abstract: false,
            kind: None,
            params: Vec::new(),
            return_type: None,
            doc_types: None,
            doc: None,
            body: None,
        }
    }

    fn class(name: &str, line: u32, end: u32) -> Container {
        Container {
            kind: "class".to_owned(),
            name: name.to_owned(),
            line,
            end_line: end,
            is_abstract: None,
            is_final: None,
            extends: None,
            implements: None,
            traits: None,
            doc: None,
        }
    }

    fn graph() -> FileGraph {
        let mut builder = crate::model::FileGraphBuilder::new("A.php");
        builder
            .container(class("A", 3, 30))
            .method(method("A", "first", 5, 10))
            .method(method("A", "second", 12, 20))
            .method(method("A", "__module__", 1, 30));
        builder.build()
    }

    #[test]
    fn a_changed_line_maps_to_the_innermost_method() {
        let found = touched(&graph(), &[(7, 8), (15, 15)], &[]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].label(), "A::first()");
        assert_eq!(found[0].span(), "L5-10");
        assert_eq!(found[0].changed, vec![(7, 8)]);
        assert_eq!(found[1].label(), "A::second()");
        assert!(found[1].changed_text().contains("L15"));
    }

    #[test]
    fn lines_outside_the_methods_fall_back_to_the_class_then_the_file() {
        // L4 is in the class body between nothing; L25 too; L2 and L40 are outside the class
        let found = touched(&graph(), &[(4, 4), (25, 26), (2, 2), (40, 40)], &[]);
        let kinds: Vec<Kind> = found.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, [Kind::Class, Kind::File]);
        assert_eq!(found[0].label(), "class A");
        assert_eq!(found[0].changed, vec![(4, 4), (25, 26)]);
        assert_eq!(found[1].changed, vec![(2, 2), (40, 40)]);
    }

    #[test]
    fn a_pure_deletion_maps_to_the_symbol_around_the_gap() {
        // lines removed after L7: inside `first`
        let found = touched(&graph(), &[], &[7]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].label(), "A::first()");
        assert!(found[0].changed.is_empty());
        assert!(found[0].changed_text().contains("L7"));
        // after L10 (the end of `first`): between two methods, so the class
        let gap = touched(&graph(), &[], &[10]);
        assert_eq!(gap[0].label(), "class A");
        // before the first line: the file
        let top = touched(&graph(), &[], &[0]);
        assert_eq!(top[0].kind, Kind::File);
    }

    #[test]
    fn a_whole_new_file_touches_every_symbol() {
        let found = touched(&graph(), &[(1, 30)], &[]);
        let labels: Vec<String> = found.iter().take(3).map(Touched::label).collect();
        assert_eq!(labels, ["class A", "A::first()", "A::second()"]);
        assert_eq!(found.len(), 4);
        assert_eq!(found[3].kind, Kind::File);
    }

    #[test]
    fn long_change_lists_are_cut() {
        let found = touched(
            &graph(),
            &[(4, 4), (11, 11), (21, 21), (25, 25), (27, 27)],
            &[],
        );
        let text = found[0].changed_text();
        assert!(text.contains("L21") && text.contains("+2"), "{text}");
        assert!(!text.contains("L25"), "{text}");
    }
}
