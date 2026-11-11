//! Előzetes szimbólum-gyűjtés: csomagnév, függvények, típusok, metódusnevek és az importok
//! (a gráfba kerülő `(alias, teljes út)` párok is).

use tree_sitter::Node;

use super::super::categorize::FileSymbols;
use super::super::parser::{ParsedGo, named_children};

/// A scan eredménye: a fájl importjai `(alias, teljes út)` párokként és a csomag neve.
pub(super) struct Scanned {
    pub imports: Vec<(String, String)>,
    pub package: Option<String>,
}

/// A szimbólumok a `symbols`-ba; visszaadja az importokat és a csomagnevet.
pub(super) fn scan(parsed: &ParsedGo<'_>, root: Node<'_>, symbols: &mut FileSymbols) -> Scanned {
    let mut out = Scanned {
        imports: Vec::new(),
        package: None,
    };
    for child in named_children(root) {
        match child.kind() {
            "package_clause" => {
                out.package = named_children(child)
                    .first()
                    .map(|n| parsed.text(*n).to_string());
            }
            "import_declaration" => {
                for spec in import_specs(child) {
                    import_spec(parsed, spec, symbols, &mut out.imports);
                }
            }
            "function_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    symbols.functions.insert(parsed.text(name).to_string());
                }
            }
            "method_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    symbols.methods.insert(parsed.text(name).to_string());
                }
            }
            "type_declaration" => type_declaration(parsed, child, symbols),
            _ => {}
        }
    }
    out
}

/// Az `import "x"` és az `import ( … )` specjei.
fn import_specs(decl: Node<'_>) -> Vec<Node<'_>> {
    let mut specs = Vec::new();
    for child in named_children(decl) {
        match child.kind() {
            "import_spec" => specs.push(child),
            "import_spec_list" => specs.extend(
                named_children(child)
                    .into_iter()
                    .filter(|s| s.kind() == "import_spec"),
            ),
            _ => {}
        }
    }
    specs
}

/// `import alias "a/b"`, `import "a/b"`, `import _ "a/b"`, `import . "a/b"`.
fn import_spec(
    parsed: &ParsedGo<'_>,
    spec: Node<'_>,
    symbols: &mut FileSymbols,
    imports: &mut Vec<(String, String)>,
) {
    let Some(path_node) = spec.child_by_field_name("path") else {
        return;
    };
    let path = unquote(parsed.text(path_node));
    if path.is_empty() {
        return;
    }
    let name = spec
        .child_by_field_name("name")
        .map(|n| parsed.text(n).to_string());
    match name.as_deref() {
        // a blank és a dot import nem köt nevet: a kulcs maga az út
        Some("_") | Some(".") => imports.push((path.clone(), path)),
        Some(alias) => {
            symbols.imports.insert(alias.to_string(), path.clone());
            imports.push((alias.to_string(), path));
        }
        None => {
            let alias = package_name(&path);
            symbols.imports.insert(alias.clone(), path.clone());
            imports.push((alias, path));
        }
    }
}

/// Típus-nevek (struct, interface, alias, definiált típus) és az interfész-metódusok nevei.
fn type_declaration(parsed: &ParsedGo<'_>, decl: Node<'_>, symbols: &mut FileSymbols) {
    for spec in named_children(decl) {
        if !matches!(spec.kind(), "type_spec" | "type_alias") {
            continue;
        }
        if let Some(name) = spec.child_by_field_name("name") {
            symbols.types.insert(parsed.text(name).to_string());
        }
        if let Some(ty) = spec
            .child_by_field_name("type")
            .filter(|t| t.kind() == "interface_type")
        {
            for elem in named_children(ty)
                .into_iter()
                .filter(|e| e.kind() == "method_elem")
            {
                if let Some(name) = elem.child_by_field_name("name") {
                    symbols.methods.insert(parsed.text(name).to_string());
                }
            }
        }
    }
}

/// Az idézőjelek (`"…"` vagy `` `…` ``) nélküli import-út.
pub(super) fn unquote(text: &str) -> String {
    text.trim_matches(|c| c == '"' || c == '`').to_string()
}

/// Az import-út alapértelmezett csomagneve: az utolsó elem; a `/v2` major-verzió az előző elemre
/// mutat, a `yaml.v3` típusú végződés `yaml`.
pub(super) fn package_name(path: &str) -> String {
    let mut parts = path.rsplit('/');
    let mut last = parts.next().unwrap_or(path);
    let major = last.len() > 1
        && last.starts_with('v')
        && last[1..].chars().all(|c| c.is_ascii_digit());
    if major && let Some(previous) = parts.next() {
        last = previous;
    }
    if let Some((stem, version)) = last.rsplit_once(".v")
        && !version.is_empty()
        && version.chars().all(|c| c.is_ascii_digit())
    {
        last = stem;
    }
    last.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_names_from_import_paths() {
        assert_eq!(package_name("fmt"), "fmt");
        assert_eq!(package_name("net/http"), "http");
        assert_eq!(package_name("github.com/a/b/v2"), "b");
        assert_eq!(package_name("gopkg.in/yaml.v3"), "yaml");
        assert_eq!(package_name("github.com/gin-gonic/gin"), "gin");
        assert_eq!(package_name("example.com/v"), "v");
    }

    #[test]
    fn unquote_handles_both_quote_kinds() {
        assert_eq!(unquote("\"fmt\""), "fmt");
        assert_eq!(unquote("`fmt`"), "fmt");
    }
}
