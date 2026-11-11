//! Parser-réteg: a tree-sitter + tree-sitter-go elrejtése (Adapter minta).
//! Tree-sitter-specifikus típus csak ide és a `visitor` alá kerülhet.

use tree_sitter::{Node, Parser, Tree};

/// A Go-forrást szintaxisfává alakító parser (cserélhető implementáció).
pub(crate) trait GoParser: Send + Sync {
    /// A forrás parse-olása; `None`, ha a parser nem tudott fát adni (pl. megszakítás).
    fn parse<'s>(&self, source: &'s str) -> Option<ParsedGo<'s>>;
}

/// Egy parse-olt Go-fájl nézete: a fa, a forrás és a sor-/szöveg-segédek.
pub(crate) struct ParsedGo<'s> {
    tree: Tree,
    source: &'s str,
}

impl<'s> ParsedGo<'s> {
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

    /// A szintaktikai hibák (ERROR / hiányzó token) dokumentum-sorrendben, legfeljebb `max` darab: 1-alapú sor
    /// (a fájlvégi hiba az utolsó tartalmas sorra) + rövid üzenet. A tree-sitter nem ad szöveges üzenetet, ezért
    /// az üzenet csak a node fajtája (pl. ``missing `)` ``), forrásszöveget nem idéz.
    pub(crate) fn errors(&self, max: usize) -> Vec<(u32, String)> {
        let root = self.root();
        let mut found = Vec::new();
        if root.has_error() {
            collect_errors(root, max, &mut found);
        }
        let last = self
            .source
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .last()
            .map(|(i, _)| i)
            .map_or(1, |i| i as u32 + 1);
        found
            .into_iter()
            .map(|node| {
                let message = if node.is_missing() {
                    format!("missing `{}`", node.kind())
                } else {
                    "unexpected syntax".to_owned()
                };
                (line(node).min(last), message)
            })
            .collect()
    }
}

/// Adapter a `tree-sitter` + `tree-sitter-go` crate-ek köré (hívásonként saját `Parser`, így szálbiztos).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TreeSitterGo;

impl GoParser for TreeSitterGo {
    fn parse<'s>(&self, source: &'s str) -> Option<ParsedGo<'s>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_go::LANGUAGE.into()).ok()?;
        let tree = parser.parse(source, None)?;
        Some(ParsedGo::new(tree, source))
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

/// Az ERROR / hiányzó node-ok gyűjtése (egy ERROR belsejébe nem megy tovább: az egy hiba).
fn collect_errors<'t>(node: Node<'t>, max: usize, out: &mut Vec<Node<'t>>) {
    if out.len() >= max {
        return;
    }
    if node.is_error() || node.is_missing() {
        out.push(node);
        return;
    }
    for child in children(node) {
        if child.has_error() || child.is_missing() {
            collect_errors(child, max, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_reports_lines() {
        let parsed = TreeSitterGo
            .parse("package a\n\ntype A struct {\n\tx int\n}\n")
            .unwrap();
        let root = parsed.root();
        assert_eq!(root.kind(), "source_file");
        let decl = named_children(root)[1];
        assert_eq!(decl.kind(), "type_declaration");
        assert_eq!((line(decl), end_line(decl)), (3, 5));
        assert!(parsed.first_error_line().is_none());
        assert!(parsed.text(decl).starts_with("type A"));
    }

    #[test]
    fn syntax_error_has_a_line() {
        let parsed = TreeSitterGo.parse("package a\n\nfunc (\n").unwrap();
        assert!(parsed.first_error_line().is_some());
    }

    #[test]
    fn errors_are_listed_with_lines_and_capped() {
        let parsed = TreeSitterGo
            .parse("package a\n\nfunc a() int {\n\treturn 1\n}\n\nfunc b( {\n}\n")
            .unwrap();
        let errors = parsed.errors(5);
        assert!(!errors.is_empty() && errors.len() <= 5);
        assert!((7..=8).contains(&errors[0].0), "{errors:?}");
        let clean = TreeSitterGo.parse("package a\n\ntype A int\n").unwrap();
        assert!(clean.errors(5).is_empty());
    }

    /// Fejlesztői segéd: `cargo test go_dump -- --ignored --nocapture` a fa S-kifejezéséhez.
    #[test]
    #[ignore]
    fn go_dump() {
        let paths = std::env::var("GO_DUMP").unwrap_or_default();
        for path in paths.split(',').filter(|p| !p.is_empty()) {
            let src = std::fs::read_to_string(path).unwrap_or_default();
            let parsed = TreeSitterGo.parse(&src).unwrap();
            println!("== {path}");
            dump(parsed.root(), &parsed, 0);
        }
    }

    fn dump(node: Node<'_>, p: &ParsedGo<'_>, depth: usize) {
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
