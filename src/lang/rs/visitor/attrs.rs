//! Attribútum-segédek: doc-komment (`///`, `//!`), láthatóság szövege, attribútumok string-literáljai.

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::{Attribute, Expr, Lit, Meta, Visibility};

/// A doc-komment első bekezdése egy sorba vonva (≤300 karakter); ha nincs: `None`.
pub(super) fn doc_of(attrs: &[Attribute]) -> Option<String> {
    let mut lines = Vec::new();
    for attr in attrs.iter().filter(|a| a.path().is_ident("doc")) {
        let Meta::NameValue(nv) = &attr.meta else {
            continue;
        };
        let Expr::Lit(lit) = &nv.value else {
            continue;
        };
        let Lit::Str(s) = &lit.lit else {
            continue;
        };
        let value = s.value();
        if value.trim().is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(value.lines().map(|l| l.trim().to_owned()));
        }
    }
    let paragraph: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .skip_while(|l| l.is_empty())
        .take_while(|l| !l.is_empty())
        .collect();
    let text = paragraph.join(" ");
    (!text.is_empty()).then(|| crate::lang::rs::parser::shorten(&text, 300))
}

/// A láthatóság a kimenet alakjában: `pub`, `pub(crate)`, `pub(super)`, `pub(in …)`, különben `private`.
pub(super) fn visibility(vis: &Visibility) -> String {
    match vis {
        Visibility::Public(_) => "pub".to_owned(),
        Visibility::Restricted(r) => {
            let path = r
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            if r.in_token.is_some() {
                format!("pub(in {path})")
            } else {
                format!("pub({path})")
            }
        }
        Visibility::Inherited => "private".to_owned(),
    }
}

/// Egy (nem doc) attribútum string-literáljai (`#[get("/users/{id}")]`, `#[command(about = "…")]`).
pub(super) fn attr_strings(attrs: &[Attribute]) -> Vec<(Span, String)> {
    let mut out = Vec::new();
    for attr in attrs.iter().filter(|a| !a.path().is_ident("doc")) {
        let tokens = match &attr.meta {
            Meta::List(list) => list.tokens.clone(),
            Meta::NameValue(nv) => quote_free_tokens(&nv.value),
            Meta::Path(_) => continue,
        };
        token_strings(tokens, &mut out);
    }
    out
}

/// A tokenfolyam (rekurzívan a csoportokban is) string-literáljai.
pub(super) fn token_strings(tokens: TokenStream, out: &mut Vec<(Span, String)>) {
    for tree in tokens {
        match tree {
            TokenTree::Group(group) => token_strings(group.stream(), out),
            TokenTree::Literal(literal) => {
                let span = literal.span();
                match Lit::new(literal) {
                    Lit::Str(s) => out.push((span, s.value())),
                    Lit::ByteStr(s) => {
                        out.push((span, String::from_utf8_lossy(&s.value()).into_owned()))
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// `#[x = "…"]` értéke: csak a literál számít (más kifejezés tokenjei nem kellenek).
fn quote_free_tokens(expr: &Expr) -> TokenStream {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(s) => TokenStream::from(TokenTree::Literal(s.token())),
            _ => TokenStream::new(),
        },
        _ => TokenStream::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(src: &str) -> syn::ItemFn {
        syn::parse_str(src).unwrap()
    }

    #[test]
    fn doc_is_first_paragraph() {
        let f = item("/// First line\n/// second.\n///\n/// Details.\nfn a() {}");
        assert_eq!(doc_of(&f.attrs).as_deref(), Some("First line second."));
        assert_eq!(doc_of(&item("fn a() {}").attrs), None);
    }

    #[test]
    fn visibility_texts() {
        assert_eq!(visibility(&item("pub fn a() {}").vis), "pub");
        assert_eq!(visibility(&item("pub(crate) fn a() {}").vis), "pub(crate)");
        assert_eq!(visibility(&item("fn a() {}").vis), "private");
    }

    #[test]
    fn attribute_string_literals() {
        let f = item("#[get(\"/users/{id}\")]\n#[doc = \"x\"]\nfn a() {}");
        let values: Vec<String> = attr_strings(&f.attrs).into_iter().map(|(_, v)| v).collect();
        assert_eq!(values, ["/users/{id}"]);
    }
}
