//! Névfeloldás: egy osztálynév → teljes név, a fájl névtere, `use`-importjai és a
//! tartalmazó osztály alapján. Mérce: `ast_graph.php` `resolveClassName` (L1240–1269).
//! Tiszta string-logika (nincs mago-típus), a kimenetben nincs vezető `\`.

use std::collections::HashMap;

/// A tartalmazó osztály (`self`/`static`/`parent` feloldásához).
#[derive(Debug, Clone, Default)]
struct ContainerCtx {
    name: String,
    /// Már feloldott `extends` lista (az első a `parent`).
    extends: Vec<String>,
}

/// Egy névtér-blokk feloldási környezete.
#[derive(Debug, Clone, Default)]
pub struct NameResolver {
    namespace: Option<String>,
    imports: HashMap<String, String>,
    container: Option<ContainerCtx>,
}

impl NameResolver {
    pub fn new(namespace: Option<String>) -> Self {
        Self {
            namespace,
            imports: HashMap::new(),
            container: None,
        }
    }

    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// `use` felvétele. Az alias kis/nagybetű-érzékeny, mint a referenciában
    /// (`$this->imports[$short]`); ugyanarra az aliasra a későbbi felülír.
    pub fn add_import(&mut self, alias: &str, full: &str) {
        self.imports.insert(alias.to_string(), full.to_string());
    }

    /// Belépés egy osztályba: `name` a rövid név, `extends` a már feloldott szülők.
    pub fn enter_container(&mut self, name: &str, extends: Vec<String>) {
        self.container = Some(ContainerCtx {
            name: name.to_string(),
            extends,
        });
    }

    /// Kilépés az osztályból (a `self`/`static`/`parent` újra feloldatlan).
    pub fn leave_container(&mut self) {
        self.container = None;
    }

    /// Osztálynév feloldása a referencia sorrendjében:
    /// 1. `\A\B` → `A\B` (teljesen minősített: nincs névtér-prefix, nincs import);
    /// 2. `self`/`static` (kis/nagybetű-független) osztályon belül → `ns\Osztály`;
    /// 3. `parent` osztályon belül → az első `extends`, vagy marad `parent`;
    /// 4. pontos import-egyezés (a teljes név az alias);
    /// 5. nem minősített név névtérben → `ns\név`;
    /// 6. minden más (minősített `A\B`; névtér nélküli név) változatlan.
    pub fn resolve_class(&self, raw: &str) -> String {
        // a referencia `Name::toString()`-je elveszíti a vezető `\`-t: az egy szegmensű név
        // innentől import / névtér szerint oldódik fel
        let raw = raw.strip_prefix('\\').unwrap_or(raw);
        if let Some(c) = &self.container {
            if raw.eq_ignore_ascii_case("self") || raw.eq_ignore_ascii_case("static") {
                return match &self.namespace {
                    Some(ns) => format!("{ns}\\{}", c.name),
                    None => c.name.clone(),
                };
            }
            if raw.eq_ignore_ascii_case("parent") {
                return c
                    .extends
                    .first()
                    .cloned()
                    .unwrap_or_else(|| raw.to_string());
            }
        }
        if let Some(full) = self.imports.get(raw) {
            return full.clone();
        }
        match &self.namespace {
            Some(ns) if !raw.contains('\\') => format!("{ns}\\{raw}"),
            _ => raw.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver() -> NameResolver {
        let mut r = NameResolver::new(Some("components".into()));
        r.add_import("Controller", "web\\Controller");
        r.add_import("Alias", "Vendor\\Pkg\\Real");
        r
    }

    fn in_class(extends: &[&str]) -> NameResolver {
        let mut r = resolver();
        r.enter_container("Me", extends.iter().map(|s| s.to_string()).collect());
        r
    }

    #[test]
    fn imported_name_resolves_to_import() {
        assert_eq!(resolver().resolve_class("Controller"), "web\\Controller");
    }

    #[test]
    fn import_alias_differs_from_class_name() {
        assert_eq!(resolver().resolve_class("Alias"), "Vendor\\Pkg\\Real");
    }

    #[test]
    fn import_lookup_is_case_sensitive_like_reference() {
        assert_eq!(
            resolver().resolve_class("controller"),
            "components\\controller"
        );
    }

    #[test]
    fn later_import_overrides_same_alias() {
        let mut r = resolver();
        r.add_import("Controller", "other\\Controller");
        assert_eq!(r.resolve_class("Controller"), "other\\Controller");
    }

    #[test]
    fn qualified_name_is_left_untouched() {
        // a referencia a backslash-t tartalmazó nevet nem prefixeli, az első szegmenst nem importálja
        assert_eq!(
            resolver().resolve_class("Controller\\Sub"),
            "Controller\\Sub"
        );
        assert_eq!(resolver().resolve_class("A\\B"), "A\\B");
    }

    #[test]
    fn qualified_name_exact_import_key_matches() {
        let mut r = resolver();
        r.add_import("A\\B", "x\\Y");
        assert_eq!(r.resolve_class("A\\B"), "x\\Y");
    }

    #[test]
    fn unimported_name_gets_namespace() {
        assert_eq!(resolver().resolve_class("Foo"), "components\\Foo");
    }

    #[test]
    fn fully_qualified_loses_leading_backslash() {
        assert_eq!(resolver().resolve_class("\\Foo\\Bar"), "Foo\\Bar");
        assert_eq!(resolver().resolve_class("\\Foo"), "components\\Foo");
    }

    #[test]
    fn single_segment_fully_qualified_resolves_like_unqualified() {
        assert_eq!(resolver().resolve_class("\\Controller"), "web\\Controller");
    }

    #[test]
    fn global_namespace_keeps_name() {
        assert_eq!(NameResolver::new(None).resolve_class("Foo"), "Foo");
        assert_eq!(NameResolver::new(None).resolve_class("A\\B"), "A\\B");
    }

    #[test]
    fn self_and_static_resolve_to_container() {
        let r = in_class(&[]);
        assert_eq!(r.resolve_class("self"), "components\\Me");
        assert_eq!(r.resolve_class("static"), "components\\Me");
        assert_eq!(r.resolve_class("SELF"), "components\\Me");
        assert_eq!(r.resolve_class("Static"), "components\\Me");
    }

    #[test]
    fn self_without_namespace_is_bare_container() {
        let mut r = NameResolver::new(None);
        r.enter_container("Me", vec![]);
        assert_eq!(r.resolve_class("self"), "Me");
    }

    #[test]
    fn parent_resolves_to_first_extends() {
        let r = in_class(&["web\\Base", "web\\Other"]);
        assert_eq!(r.resolve_class("parent"), "web\\Base");
        assert_eq!(r.resolve_class("PARENT"), "web\\Base");
    }

    #[test]
    fn parent_without_extends_stays_as_written() {
        let r = in_class(&[]);
        assert_eq!(r.resolve_class("parent"), "parent");
        assert_eq!(r.resolve_class("Parent"), "Parent");
    }

    #[test]
    fn self_outside_container_falls_through_like_reference() {
        assert_eq!(resolver().resolve_class("self"), "components\\self");
        assert_eq!(resolver().resolve_class("parent"), "components\\parent");
    }

    #[test]
    fn leave_container_resets_self() {
        let mut r = in_class(&["web\\Base"]);
        r.leave_container();
        assert_eq!(r.resolve_class("self"), "components\\self");
    }

    #[test]
    fn enter_container_replaces_previous() {
        let mut r = in_class(&["web\\Base"]);
        r.enter_container("Next", vec!["x\\Y".into()]);
        assert_eq!(r.resolve_class("self"), "components\\Next");
        assert_eq!(r.resolve_class("parent"), "x\\Y");
    }

    #[test]
    fn namespace_accessor() {
        assert_eq!(resolver().namespace(), Some("components"));
        assert_eq!(NameResolver::new(None).namespace(), None);
    }
}
