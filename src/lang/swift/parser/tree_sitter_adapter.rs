//! `TreeSitterSwift`: a `SwiftParser` tree-sitter-swift alapú implementációja.

use tree_sitter::Parser;

use super::{ParsedSwift, SwiftParser};

/// Adapter a `tree-sitter` + `tree-sitter-swift` crate-ek köré (hívásonként saját `Parser`, így szálbiztos).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TreeSitterSwift;

impl SwiftParser for TreeSitterSwift {
    fn parse<'s>(&self, source: &'s str) -> Option<ParsedSwift<'s>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_swift::LANGUAGE.into())
            .ok()?;
        let tree = parser.parse(source, None)?;
        Some(ParsedSwift::new(tree, source))
    }
}
