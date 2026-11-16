//! `TreeSitterKotlin`: a `KotlinParser` tree-sitter-kotlin-ng alapú implementációja.

use tree_sitter::Parser;

use super::{KotlinParser, ParsedKotlin};

/// Adapter a `tree-sitter` + `tree-sitter-kotlin-ng` crate-ek köré (hívásonként saját `Parser`, így szálbiztos).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TreeSitterKotlin;

impl KotlinParser for TreeSitterKotlin {
    fn parse<'s>(&self, source: &'s str) -> Option<ParsedKotlin<'s>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
            .ok()?;
        let tree = parser.parse(source, None)?;
        Some(ParsedKotlin::new(tree, source))
    }
}
