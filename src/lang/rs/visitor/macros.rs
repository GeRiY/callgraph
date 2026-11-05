//! Makró-argumentumok: a makrók nincsenek kibontva, de a törzsük gyakran kifejezés-lista
//! (`println!("{}", f(x))`, `vec![a, b]`) vagy utasítás-sorozat (`thread_local! { static X: … = …; }`).
//! Ha egyik sem, a törzs átlátszatlan (csak a string-literálok gyűjthetők a tokenekből).

use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Block, Expr, Macro, Stmt, Token};

/// A makró törzse parse-olva.
pub(super) enum MacroBody {
    Exprs(Vec<Expr>),
    Stmts(Vec<Stmt>),
    Opaque,
}

/// A törzs értelmezése: előbb vesszős kifejezés-lista, utána utasítás-sorozat.
pub(super) fn parse_body(mac: &Macro) -> MacroBody {
    if mac.tokens.is_empty() {
        return MacroBody::Exprs(Vec::new());
    }
    if let Ok(list) = mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
        return MacroBody::Exprs(list.into_iter().collect());
    }
    match mac.parse_body_with(Block::parse_within) {
        Ok(stmts) => MacroBody::Stmts(stmts),
        Err(_) => MacroBody::Opaque,
    }
}

/// A parse-olható makró-törzs bejárása ugyanazzal a visitorral; `false`, ha a törzs átlátszatlan.
pub(super) fn visit_body<V: for<'x> Visit<'x>>(visitor: &mut V, mac: &Macro) -> bool {
    match parse_body(mac) {
        MacroBody::Exprs(exprs) => {
            for expr in &exprs {
                visitor.visit_expr(expr);
            }
            true
        }
        MacroBody::Stmts(stmts) => {
            for stmt in &stmts {
                visitor.visit_stmt(stmt);
            }
            true
        }
        MacroBody::Opaque => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(src: &str) -> MacroBody {
        let mac: Macro = syn::parse_str(src).unwrap();
        parse_body(&mac)
    }

    #[test]
    fn expression_lists_statements_and_opaque() {
        assert!(matches!(body("println!(\"{}\", f(x))"), MacroBody::Exprs(e) if e.len() == 2));
        assert!(matches!(body("vec![0; n]"), MacroBody::Stmts(s) if s.len() == 2));
        assert!(matches!(
            body("thread_local! { static X: u8 = f(); }"),
            MacroBody::Stmts(_)
        ));
        assert!(matches!(body("json!({ \"a\": 1 })"), MacroBody::Opaque));
    }
}
