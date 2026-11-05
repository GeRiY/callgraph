//! Swift hívás-kategorizálás (Chain of Responsibility a `core::Chain`-en): parser-független `CallInfo`-ból.

use std::collections::{HashMap, HashSet};

use super::known::{
    NATIVE_FUNCTIONS, NATIVE_METHODS, NATIVE_TYPES, SWIFTUI_FUNCTIONS, SWIFTUI_MODIFIERS,
    SWIFTUI_VIEWS, is_in,
};
use crate::core::{Chain, Rule};

/// Egy hívás leírása a kategorizáláshoz.
#[derive(Debug, Clone, Default)]
pub(crate) struct CallInfo {
    /// A hívott név (az utolsó tag): `padding`, `load`, `Text`.
    pub name: String,
    /// Tag-hívás (`a.b()`, `.padding()`), nem puszta azonosító.
    pub member: bool,
    /// A lánc bal szélső azonosítója (`viewModel.load()` → `viewModel`; `JSONDecoder().decode` → `JSONDecoder`).
    pub root: Option<String>,
    /// A fogadó `self` / `Self`.
    pub on_self: bool,
    /// A fogadó maga is hívás vagy implicit tag (módosító-lánc: `Text("a").font(.x)`, `.padding()`).
    pub chained: bool,
    /// A hívás egy nézet `body`-jában / `some View` computed property-ben van.
    pub in_view: bool,
}

/// A fájl ismert nevei: a típusok (nézet-e) és a fájlban definiált függvények.
#[derive(Debug, Default)]
pub(crate) struct FileSymbols {
    /// Típusnév → SwiftUI-nézet-e.
    pub types: HashMap<String, bool>,
    /// A fájlban definiált függvény- és metódusnevek.
    pub functions: HashSet<String>,
    /// A fájlban deklarált protokollok.
    pub protocols: HashSet<String>,
}

/// A kategorizáló bemenete: a hívás és a fájl szimbólumai.
pub(crate) struct CategorizeInput<'a> {
    pub call: &'a CallInfo,
    pub symbols: &'a FileSymbols,
}

type SwiftRule = dyn for<'a> Rule<CategorizeInput<'a>, Output = &'static str> + Send + Sync;

/// A hívás-kategorizáló lánc (az első illeszkedő szabály dönt; tartalék: `external_call`).
pub(crate) struct Categorizer {
    chain: Chain<SwiftRule, &'static str>,
}

impl Default for Categorizer {
    fn default() -> Self {
        Self {
            chain: Chain::new(
                vec![
                    Box::new(Internal),
                    Box::new(Constructor),
                    Box::new(FreeFunction),
                    Box::new(Modifier),
                    Box::new(NativeMember),
                ],
                "external_call",
            ),
        }
    }
}

impl Categorizer {
    /// A hívás kategóriája.
    pub(crate) fn categorize(&self, call: &CallInfo, symbols: &FileSymbols) -> &'static str {
        self.chain.resolve(&CategorizeInput { call, symbols })
    }
}

fn is_type_name(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// `self.x()` vagy a fájlban definiált függvény puszta hívása → `internal_helper`.
struct Internal;
impl Rule<CategorizeInput<'_>> for Internal {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        let local = i.symbols.functions.contains(&c.name);
        ((c.on_self && local) || (!c.member && !is_type_name(&c.name) && local))
            .then_some("internal_helper")
    }
}

/// Típus-konstruktor (`Text("a")`, `Todo(...)`, `TodoRow(...)`).
struct Constructor;
impl Rule<CategorizeInput<'_>> for Constructor {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        if c.member || !is_type_name(&c.name) {
            return None;
        }
        Some(match i.symbols.types.get(&c.name) {
            Some(true) => "component_use",
            Some(false) => "instantiation",
            None if is_in(SWIFTUI_VIEWS, &c.name) => "swiftui_view",
            None if is_in(NATIVE_TYPES, &c.name) => "native_swift",
            None if c.in_view => "component_use",
            None => "instantiation",
        })
    }
}

/// Puszta, kisbetűs függvényhívás (nem a fájlban definiált).
struct FreeFunction;
impl Rule<CategorizeInput<'_>> for FreeFunction {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        if c.member {
            return None;
        }
        Some(if is_in(SWIFTUI_FUNCTIONS, &c.name) {
            "swiftui_api"
        } else if is_in(NATIVE_FUNCTIONS, &c.name) {
            "native_swift"
        } else {
            "global_helper"
        })
    }
}

/// SwiftUI-módosító: ismert név egy lánc / implicit tag / SwiftUI-nézet / nézet-törzs fogadón.
struct Modifier;
impl Rule<CategorizeInput<'_>> for Modifier {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        let view_root = c
            .root
            .as_deref()
            .is_some_and(|r| is_in(SWIFTUI_VIEWS, r) || i.symbols.types.get(r) == Some(&true));
        (is_in(SWIFTUI_MODIFIERS, &c.name) && (c.chained || view_root || c.in_view))
            .then_some("swiftui_modifier")
    }
}

/// Natív típus statikus / példány-metódusa, vagy ismert stdlib-metódus bármilyen fogadón.
struct NativeMember;
impl Rule<CategorizeInput<'_>> for NativeMember {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        let native_root = c
            .root
            .as_deref()
            .is_some_and(|r| is_in(NATIVE_TYPES, r) && !i.symbols.types.contains_key(r));
        let own_method = c.on_self && i.symbols.functions.contains(&c.name);
        (!own_method && (native_root || is_in(NATIVE_METHODS, &c.name))).then_some("native_swift")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols() -> FileSymbols {
        let mut s = FileSymbols::default();
        s.types.insert("TodoRow".into(), true);
        s.types.insert("Todo".into(), false);
        s.functions.insert("save".into());
        s
    }

    fn cat(call: CallInfo) -> &'static str {
        Categorizer::default().categorize(&call, &symbols())
    }

    fn bare(name: &str) -> CallInfo {
        CallInfo {
            name: name.into(),
            ..CallInfo::default()
        }
    }

    fn member(root: &str, name: &str) -> CallInfo {
        CallInfo {
            name: name.into(),
            member: true,
            root: Some(root.into()),
            on_self: root == "self",
            ..CallInfo::default()
        }
    }

    #[test]
    fn local_functions_are_internal() {
        assert_eq!(cat(bare("save")), "internal_helper");
        assert_eq!(cat(member("self", "save")), "internal_helper");
    }

    #[test]
    fn constructors_by_kind() {
        assert_eq!(cat(bare("TodoRow")), "component_use");
        assert_eq!(cat(bare("Todo")), "instantiation");
        assert_eq!(cat(bare("VStack")), "swiftui_view");
        assert_eq!(cat(bare("UUID")), "native_swift");
        assert_eq!(cat(bare("Unknown")), "instantiation");
        let in_view = CallInfo {
            in_view: true,
            ..bare("Unknown")
        };
        assert_eq!(cat(in_view), "component_use");
    }

    #[test]
    fn free_functions() {
        assert_eq!(cat(bare("print")), "native_swift");
        assert_eq!(cat(bare("withAnimation")), "swiftui_api");
        assert_eq!(cat(bare("helper")), "global_helper");
    }

    #[test]
    fn members() {
        let chained = CallInfo {
            chained: true,
            ..member("Text", "padding")
        };
        assert_eq!(cat(chained), "swiftui_modifier");
        assert_eq!(cat(member("DispatchQueue", "async")), "native_swift");
        assert_eq!(cat(member("todos", "append")), "native_swift");
        assert_eq!(cat(member("viewModel", "load")), "external_call");
    }
}
