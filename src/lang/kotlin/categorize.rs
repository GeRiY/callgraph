//! Kotlin hívás-kategorizálás (Chain of Responsibility a `core::Chain`-en): parser-független `CallInfo`-ból.

use std::collections::{HashMap, HashSet};

use super::known::{
    ANDROID_METHODS, ANDROID_TYPES, COMPOSE_API, COMPOSE_MODIFIERS, COMPOSE_VIEWS, COROUTINES_API,
    NATIVE_FUNCTIONS, NATIVE_METHODS, NATIVE_TYPES, is_in,
};
use crate::core::{Chain, Rule};

/// Egy hívás leírása a kategorizáláshoz.
#[derive(Debug, Clone, Default)]
pub(crate) struct CallInfo {
    /// A hívott név (az utolsó tag): `padding`, `load`, `Text`.
    pub name: String,
    /// Tag-hívás (`a.b()`), nem puszta azonosító.
    pub member: bool,
    /// A lánc bal szélső azonosítója (`viewModel.load()` → `viewModel`; `Modifier.padding()` → `Modifier`).
    pub root: Option<String>,
    /// A fogadó `this` (vagy a hívás implicit-this).
    pub on_self: bool,
    /// A fogadó maga is hívás (módosító-lánc: `Modifier.padding(8.dp).fillMaxWidth()`).
    pub chained: bool,
    /// A hívás `@Composable` függvény törzsében van.
    pub in_compose: bool,
    /// Az első argumentum string-literál (`composable("home") { }`).
    pub first_arg_string: bool,
    /// A fogadó ismert típusa (helyi `val`, property, konstruktor-paraméter), ha feloldható.
    pub receiver_type: Option<String>,
}

/// A fájl ismert nevei: a típusok, a fájlban definiált függvények és a `@Composable` függvények.
#[derive(Debug, Default)]
pub(crate) struct FileSymbols {
    /// A fájlban deklarált típusok (class / object / interface) nevei.
    pub types: HashSet<String>,
    /// A fájlban definiált függvény- és metódusnevek.
    pub functions: HashSet<String>,
    /// A fájlban definiált `@Composable` függvények nevei.
    pub composables: HashSet<String>,
    /// Osztály → (property / konstruktor-paraméter neve → típusa).
    pub members: HashMap<String, HashMap<String, String>>,
    /// A `composable("útvonal") { Képernyő() }` párok: (útvonal, képernyő-függvény neve).
    pub routes: Vec<(String, String)>,
    /// A képernyő-hívások node-azonosítója → útvonaluk (a hívás `nav` fajtát kap).
    pub screen_nodes: HashMap<usize, String>,
}

/// A kategorizáló bemenete: a hívás és a fájl szimbólumai.
pub(crate) struct CategorizeInput<'a> {
    pub call: &'a CallInfo,
    pub symbols: &'a FileSymbols,
}

type KotlinRule = dyn for<'a> Rule<CategorizeInput<'a>, Output = &'static str> + Send + Sync;

/// A hívás-kategorizáló lánc (az első illeszkedő szabály dönt; tartalék: `external_call`).
pub(crate) struct Categorizer {
    chain: Chain<KotlinRule, &'static str>,
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
                    Box::new(AndroidMember),
                    Box::new(FrameworkMember),
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

/// A fogadó effektív típusa: az ismert típus, különben a lánc gyökere (statikus hívás: `Log.d`).
fn receiver(c: &CallInfo) -> Option<&str> {
    c.receiver_type.as_deref().or(c.root.as_deref())
}

/// `this.x()` vagy a fájlban definiált kisbetűs függvény puszta hívása → `internal_helper`.
struct Internal;
impl Rule<CategorizeInput<'_>> for Internal {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        let local = i.symbols.functions.contains(&c.name);
        ((c.on_self && local && !is_type_name(&c.name))
            || (!c.member && !is_type_name(&c.name) && local))
            .then_some("internal_helper")
    }
}

/// Típus-konstruktor vagy `@Composable` hívás (`Text("a")`, `Todo(..)`, `TodoRow(..)`).
struct Constructor;
impl Rule<CategorizeInput<'_>> for Constructor {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        if c.member || !is_type_name(&c.name) {
            return None;
        }
        let n = c.name.as_str();
        Some(if i.symbols.composables.contains(n) {
            "component_use"
        } else if i.symbols.types.contains(n) {
            "instantiation"
        } else if is_in(COMPOSE_VIEWS, n) {
            "compose_view"
        } else if is_in(COMPOSE_API, n) {
            "compose_api"
        } else if is_in(COROUTINES_API, n) {
            "coroutines_api"
        } else if is_in(NATIVE_TYPES, n) {
            "native_kotlin"
        } else if is_in(ANDROID_TYPES, n) {
            "android_api"
        } else if c.in_compose {
            "component_use"
        } else {
            "instantiation"
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
        let n = c.name.as_str();
        // navigációs útvonal-deklaráció: `composable("home") { }`, `navigation("graph") { }`
        if c.first_arg_string && matches!(n, "composable" | "navigation" | "dialog") {
            return Some("compose_api");
        }
        Some(if is_in(COMPOSE_API, n) {
            "compose_api"
        } else if is_in(COROUTINES_API, n) {
            "coroutines_api"
        } else if is_in(NATIVE_FUNCTIONS, n) || is_in(NATIVE_METHODS, n) {
            "native_kotlin"
        } else if is_in(ANDROID_METHODS, n) {
            "android_api"
        } else {
            "global_helper"
        })
    }
}

/// Compose-módosító: ismert név egy `Modifier` gyökerű vagy láncolt hívásban.
struct Modifier;
impl Rule<CategorizeInput<'_>> for Modifier {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        let modifier_root = c
            .root
            .as_deref()
            .is_some_and(|r| r.to_ascii_lowercase().ends_with("modifier"));
        (is_in(COMPOSE_MODIFIERS, &c.name) && (c.chained || modifier_root))
            .then_some("compose_modifier")
    }
}

/// Android-keretrendszer: típus-gyökerű hívás (`Log.d`, `Toast.makeText`) vagy ismert Android-metódus.
struct AndroidMember;
impl Rule<CategorizeInput<'_>> for AndroidMember {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        let typed =
            receiver(c).is_some_and(|r| is_in(ANDROID_TYPES, r) && !i.symbols.types.contains(r));
        (typed || is_in(ANDROID_METHODS, &c.name)).then_some("android_api")
    }
}

/// Compose-állapot (`x.collectAsState()`) és coroutines-művelet (`scope.launch { }`, `flow.stateIn(..)`).
struct FrameworkMember;
impl Rule<CategorizeInput<'_>> for FrameworkMember {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        if i.symbols.functions.contains(&c.name) && c.on_self {
            return None;
        }
        if is_in(COMPOSE_API, &c.name) {
            Some("compose_api")
        } else if is_in(COROUTINES_API, &c.name) {
            Some("coroutines_api")
        } else {
            None
        }
    }
}

/// Natív típus statikus / példány-metódusa, vagy ismert stdlib-metódus bármilyen fogadón.
struct NativeMember;
impl Rule<CategorizeInput<'_>> for NativeMember {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        let native_root =
            receiver(c).is_some_and(|r| is_in(NATIVE_TYPES, r) && !i.symbols.types.contains(r));
        let own_method = c.on_self && i.symbols.functions.contains(&c.name);
        (!own_method && (native_root || is_in(NATIVE_METHODS, &c.name))).then_some("native_kotlin")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols() -> FileSymbols {
        let mut s = FileSymbols::default();
        s.types.insert("Todo".into());
        s.composables.insert("TodoRow".into());
        s.functions.insert("save".into());
        s.functions.insert("TodoRow".into());
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
            on_self: root == "this",
            ..CallInfo::default()
        }
    }

    #[test]
    fn local_functions_are_internal() {
        assert_eq!(cat(bare("save")), "internal_helper");
        assert_eq!(cat(member("this", "save")), "internal_helper");
    }

    #[test]
    fn constructors_by_kind() {
        assert_eq!(cat(bare("TodoRow")), "component_use");
        assert_eq!(cat(bare("Todo")), "instantiation");
        assert_eq!(cat(bare("Column")), "compose_view");
        assert_eq!(cat(bare("ArrayList")), "native_kotlin");
        assert_eq!(cat(bare("Intent")), "android_api");
        assert_eq!(cat(bare("Unknown")), "instantiation");
        let in_compose = CallInfo {
            in_compose: true,
            ..bare("Unknown")
        };
        assert_eq!(cat(in_compose), "component_use");
    }

    #[test]
    fn free_functions() {
        assert_eq!(cat(bare("println")), "native_kotlin");
        assert_eq!(cat(bare("remember")), "compose_api");
        assert_eq!(cat(bare("launch")), "coroutines_api");
        assert_eq!(cat(bare("helper")), "global_helper");
    }

    #[test]
    fn members() {
        assert_eq!(cat(member("Modifier", "padding")), "compose_modifier");
        let chained = CallInfo {
            chained: true,
            ..member("Modifier", "fillMaxWidth")
        };
        assert_eq!(cat(chained), "compose_modifier");
        assert_eq!(cat(member("Log", "d")), "android_api");
        assert_eq!(cat(member("viewModelScope", "launch")), "coroutines_api");
        assert_eq!(cat(member("state", "collectAsState")), "compose_api");
        assert_eq!(cat(member("todos", "map")), "native_kotlin");
        assert_eq!(cat(member("repo", "load")), "external_call");
        let typed = CallInfo {
            receiver_type: Some("String".into()),
            ..member("name", "slug")
        };
        assert_eq!(cat(typed), "native_kotlin");
    }
}
