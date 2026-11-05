//! Parser-réteg: a tree-sitter + tree-sitter-swift elrejtése (Adapter minta).
//! Tree-sitter-specifikus típus csak ide és a `visitor` alá kerülhet.

mod tree_sitter_adapter;

pub(crate) use tree_sitter_adapter::TreeSitterSwift;

use tree_sitter::{Node, Tree};

/// A Swift-forrást szintaxisfává alakító parser (cserélhető implementáció).
pub(crate) trait SwiftParser: Send + Sync {
    /// A forrás parse-olása; `None`, ha a parser nem tudott fát adni (pl. megszakítás).
    fn parse<'s>(&self, source: &'s str) -> Option<ParsedSwift<'s>>;
}

/// Egy parse-olt Swift-fájl nézete: a fa, a forrás és a sor-/szöveg-segédek.
pub(crate) struct ParsedSwift<'s> {
    tree: Tree,
    source: &'s str,
}

impl<'s> ParsedSwift<'s> {
    pub(crate) fn new(tree: Tree, source: &'s str) -> Self {
        Self { tree, source }
    }

    /// A fa gyökere (`source_file`).
    pub(crate) fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// Egy node forrásszövege.
    pub(crate) fn text(&self, node: Node<'_>) -> &'s str {
        self.source.get(node.byte_range()).unwrap_or_default()
    }

    /// Az első szintaktikai hiba (ERROR / hiányzó token) 1-alapú sora, ha van.
    pub(crate) fn first_error_line(&self) -> Option<u32> {
        let root = self.root();
        if !root.has_error() {
            return None;
        }
        first_error(root).map(line)
    }
}

/// Egy node kezdősora (1-alapú).
pub(crate) fn line(node: Node<'_>) -> u32 {
    node.start_position().row as u32 + 1
}

/// Egy node záró sora (1-alapú; a sor eleji záró pozíció az előző sorhoz tartozik).
pub(crate) fn end_line(node: Node<'_>) -> u32 {
    let end = node.end_position();
    let row = if end.column == 0 && end.row > node.start_position().row {
        end.row - 1
    } else {
        end.row
    };
    row as u32 + 1
}

/// A node közvetlen gyerekei (névtelen tokenekkel együtt).
pub(crate) fn children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// A node közvetlen nevesített gyerekei.
pub(crate) fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn first_error(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    children(node)
        .into_iter()
        .filter(|c| c.has_error())
        .find_map(first_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_reports_lines() {
        let parsed = TreeSitterSwift
            .parse("struct A {\n  let x = 1\n}\n")
            .unwrap();
        let root = parsed.root();
        assert_eq!(root.kind(), "source_file");
        let decl = named_children(root)[0];
        assert_eq!((line(decl), end_line(decl)), (1, 3));
        assert!(parsed.first_error_line().is_none());
        assert!(parsed.text(decl).starts_with("struct A"));
    }

    #[test]
    fn syntax_error_has_a_line() {
        let parsed = TreeSitterSwift.parse("struct A {\n  func (\n").unwrap();
        assert!(parsed.first_error_line().is_some());
    }

    /// Fejlesztői segéd: `cargo test swift_dump -- --ignored --nocapture` a fa S-kifejezéséhez.
    #[test]
    #[ignore]
    fn swift_dump() {
        let paths = std::env::var("SWIFT_DUMP").unwrap_or_default();
        for path in paths.split(',').filter(|p| !p.is_empty()) {
            let src = std::fs::read_to_string(path).unwrap_or_default();
            let parsed = TreeSitterSwift.parse(&src).unwrap();
            println!("== {path}");
            dump(parsed.root(), &parsed, 0);
        }
    }

    fn dump(node: Node<'_>, p: &ParsedSwift<'_>, depth: usize) {
        if !node.is_named() {
            return;
        }
        let text: String = p.text(node).chars().take(40).collect();
        println!(
            "{}{} L{} {:?}",
            "  ".repeat(depth),
            node.kind(),
            line(node),
            text.replace('\n', "⏎")
        );
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if let Some(name) = cursor.field_name() {
                    print!("{}[{name}]", "  ".repeat(depth + 1));
                    println!();
                }
                dump(child, p, depth + 1);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
}
