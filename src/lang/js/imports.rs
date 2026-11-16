//! Top-level `import` deklarációk: a fájl `imports` térképe és a hívás-feloldás import-kötései.

use std::sync::Arc;

use oxc_ast::ast::{ImportDeclarationSpecifier, Program, Statement};

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::resolve::Resolver;

/// Egy importált név a hívás-feloldáshoz (a JS `bindings` térkép egy eleme).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBinding {
    pub local: String,
    /// `<display>::<importált vagy helyi név>`.
    pub resolved: String,
    pub category: &'static str,
    /// A forrás `.vue` fájl.
    pub component: bool,
    /// Az `imports` térkép értéke: `<display>` / `::*` / `::<importált>`.
    pub import_value: String,
    /// A forrás-specifikátor, ha nem helyi (se nem relatív, se nem alias-os): a hívások `Call::from`-ja.
    pub package: Option<String>,
}

/// A fájl top-level importjai kötésekké feloldva, forrásbeli sorrendben.
pub fn import_bindings(
    program: &Program<'_>,
    ctx: &PassContext<'_>,
    resolver: &Resolver,
) -> Vec<ImportBinding> {
    let mut out = Vec::new();
    for stmt in &program.body {
        let Statement::ImportDeclaration(decl) = stmt else {
            continue;
        };
        let source = decl.source.value.display().to_string();
        let source = source.as_str();
        let resolved = resolver.resolve(source, ctx.path);
        for spec in decl.specifiers.iter().flatten() {
            let (local, imported) = match spec {
                ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                    (s.local.name.to_string(), "default".to_string())
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                    (s.local.name.to_string(), "*".to_string())
                }
                ImportDeclarationSpecifier::ImportSpecifier(s) => {
                    (s.local.name.to_string(), s.imported.name().to_string())
                }
            };
            let plain = imported == "default" || imported == "*";
            let suffix = match imported.as_str() {
                "default" => String::new(),
                "*" => "::*".to_string(),
                name => format!("::{name}"),
            };
            let cat_name = if plain {
                local.as_str()
            } else {
                imported.as_str()
            };
            out.push(ImportBinding {
                resolved: format!(
                    "{}::{}",
                    resolved.display,
                    if plain { &local } else { &imported }
                ),
                category: categorize(source, cat_name, resolved.local),
                component: resolved.display.ends_with(".vue"),
                import_value: format!("{}{suffix}", resolved.display),
                package: (!resolved.local).then(|| source.to_owned()),
                local,
            });
        }
    }
    out
}

/// Az import forrásából a hívás-kategória (`categorize`).
pub fn categorize(source: &str, imported: &str, local: bool) -> &'static str {
    if source == "vue" {
        return "vue_api";
    }
    if source.starts_with("@inertiajs/") {
        return "inertia";
    }
    if source == "vue-i18n" || source == "laravel-vue-i18n" {
        return "i18n_helper";
    }
    if source.contains("ziggy") {
        return "route_helper";
    }
    if source == "axios" || source.ends_with("AxiosHandler.js") {
        return "http_call";
    }
    if is_use_name(imported) || (local && has_composables_dir(source)) {
        return "composable_use";
    }
    if local { "module_use" } else { "package_use" }
}

/// `/^use[A-Z]/`.
fn is_use_name(name: &str) -> bool {
    name.strip_prefix("use")
        .and_then(|rest| rest.chars().next())
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// `/\/Composables?\//i`.
fn has_composables_dir(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.contains("/composable/") || lower.contains("/composables/")
}

/// Az `imports` mezőt töltő passz.
pub struct ImportsPass {
    resolver: Arc<Resolver>,
}

impl ImportsPass {
    pub fn new(resolver: Arc<Resolver>) -> Self {
        Self { resolver }
    }
}

impl JsPass for ImportsPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        for b in import_bindings(program, ctx, &self.resolver) {
            out.builder.import(b.local, b.import_value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorizes_like_the_reference() {
        assert_eq!(categorize("vue", "ref", false), "vue_api");
        assert_eq!(categorize("@inertiajs/vue3", "router", false), "inertia");
        assert_eq!(categorize("ziggy-js", "route", false), "route_helper");
        assert_eq!(categorize("axios", "axios", false), "http_call");
        assert_eq!(
            categorize("resources/js/Composables/a.js", "x", true),
            "composable_use"
        );
        assert_eq!(categorize("pkg", "useFoo", false), "composable_use");
        assert_eq!(categorize("pkg", "usefoo", false), "package_use");
        assert_eq!(categorize("a/b.js", "x", true), "module_use");
    }
}
