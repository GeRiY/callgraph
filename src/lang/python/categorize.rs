//! Python hívás-kategorizálás (Chain of Responsibility a `core::Chain`-en): parser-független `CallInfo`-ból.

use std::collections::{HashMap, HashSet};

use super::known::{BUILTINS, NATIVE_METHODS, STDLIB_MODULES, is_in};
use crate::core::{Chain, Rule};

/// Egy hívás leírása a kategorizáláshoz.
#[derive(Debug, Clone, Default)]
pub(crate) struct CallInfo {
    /// A hívott név (az utolsó tag): `join`, `Foo`, `print`.
    pub name: String,
    /// Tag-hívás (`a.b()`), nem puszta azonosító.
    pub member: bool,
    /// A lánc bal szélső azonosítója (`os.path.join()` → `os`; `super().f()` → `super`).
    pub root: Option<String>,
    /// A fogadó pontosan `self` / `cls`.
    pub on_self: bool,
    /// A fogadó maga is hívás (`a().b()`).
    pub chained: bool,
}

/// A fájl ismert nevei: modul-függvények, osztályok, metódusok és az importált nevek.
#[derive(Debug, Default)]
pub(crate) struct FileSymbols {
    /// Modul-szintű függvények.
    pub functions: HashSet<String>,
    /// Osztályok (beágyazottak is).
    pub classes: HashSet<String>,
    /// Osztályokban definiált metódusnevek.
    pub methods: HashSet<String>,
    /// Importált kötött név → teljes út (`import a.b as c` → `c: a.b`; `from a import b` → `b: a.b`).
    pub imports: HashMap<String, String>,
}

/// A kategorizáló bemenete: a hívás és a fájl szimbólumai.
pub(crate) struct CategorizeInput<'a> {
    pub call: &'a CallInfo,
    pub symbols: &'a FileSymbols,
}

type PyRule = dyn for<'a> Rule<CategorizeInput<'a>, Output = &'static str> + Send + Sync;

/// A hívás-kategorizáló lánc (az első illeszkedő szabály dönt; tartalék: `external_call`).
pub(crate) struct Categorizer {
    chain: Chain<PyRule, &'static str>,
}

impl Default for Categorizer {
    fn default() -> Self {
        Self {
            chain: Chain::new(
                vec![
                    Box::new(OnSelf),
                    Box::new(Bare),
                    Box::new(ClassMember),
                    Box::new(ModuleMember),
                    Box::new(NativeMethod),
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

fn is_class_name(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// A bekötött név gyökérmodulja (`os.path` → `os`); relatív importnál (`.models.User`) `None`.
fn root_module(full: &str) -> Option<&str> {
    (!full.starts_with('.')).then(|| full.split('.').next().unwrap_or(full))
}

fn is_stdlib(full: &str) -> bool {
    root_module(full).is_some_and(|m| is_in(STDLIB_MODULES, m))
}

/// `self.x()` / `cls.x()`: az osztály (vagy ősei) metódusa; `super().x()` ős-hívás.
struct OnSelf;
impl Rule<CategorizeInput<'_>> for OnSelf {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        if c.on_self {
            return Some(if i.symbols.methods.contains(&c.name) {
                "internal_helper"
            } else {
                "external_call"
            });
        }
        (c.chained && c.root.as_deref() == Some("super")).then_some("external_call")
    }
}

/// Puszta hívás: fájlbeli függvény / osztály, importált név, beépített, ismeretlen.
struct Bare;
impl Rule<CategorizeInput<'_>> for Bare {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        if c.member {
            return None;
        }
        let s = i.symbols;
        Some(if s.classes.contains(&c.name) {
            "instantiation"
        } else if s.functions.contains(&c.name) {
            "internal_helper"
        } else if let Some(full) = s.imports.get(&c.name) {
            if is_class_name(&c.name) {
                "instantiation"
            } else if is_stdlib(full) {
                "native_python"
            } else {
                "external_call"
            }
        } else if is_in(BUILTINS, &c.name) {
            "native_python"
        } else if is_class_name(&c.name) {
            "instantiation"
        } else {
            "global_helper"
        })
    }
}

/// `Osztály.metódus()` a fájlban definiált osztályon: statikus / osztály-szintű hívás.
struct ClassMember;
impl Rule<CategorizeInput<'_>> for ClassMember {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let root = i.call.root.as_deref()?;
        (i.symbols.classes.contains(root) && !i.call.chained).then_some("static_call")
    }
}

/// `modul.függvény()` importált modulon: stdlib → `native_python`, más → `external_call`.
struct ModuleMember;
impl Rule<CategorizeInput<'_>> for ModuleMember {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let root = i.call.root.as_deref()?;
        let full = i.symbols.imports.get(root)?;
        if i.call.chained {
            return None;
        }
        Some(if is_stdlib(full) {
            "native_python"
        } else {
            "external_call"
        })
    }
}

/// Beépített típus gyakori metódusa (`items`, `append`, `join`…) bármilyen fogadón.
struct NativeMethod;
impl Rule<CategorizeInput<'_>> for NativeMethod {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        is_in(NATIVE_METHODS, &i.call.name).then_some("native_python")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols() -> FileSymbols {
        let mut s = FileSymbols::default();
        s.functions.insert("helper".into());
        s.classes.insert("Repo".into());
        s.methods.insert("save".into());
        s.imports.insert("os".into(), "os".into());
        s.imports.insert("np".into(), "numpy".into());
        s.imports.insert("Client".into(), "httpx.Client".into());
        s.imports.insert("fetch".into(), "net.fetch".into());
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
            on_self: root == "self" || root == "cls",
            ..CallInfo::default()
        }
    }

    #[test]
    fn bare_calls() {
        assert_eq!(cat(bare("helper")), "internal_helper");
        assert_eq!(cat(bare("Repo")), "instantiation");
        assert_eq!(cat(bare("Client")), "instantiation");
        assert_eq!(cat(bare("fetch")), "external_call");
        assert_eq!(cat(bare("print")), "native_python");
        assert_eq!(cat(bare("ValueError")), "native_python");
        assert_eq!(cat(bare("mystery")), "global_helper");
        assert_eq!(cat(bare("Unknown")), "instantiation");
    }

    #[test]
    fn members() {
        assert_eq!(cat(member("self", "save")), "internal_helper");
        assert_eq!(cat(member("cls", "other")), "external_call");
        assert_eq!(cat(member("Repo", "create")), "static_call");
        assert_eq!(cat(member("os", "getcwd")), "native_python");
        assert_eq!(cat(member("np", "array")), "external_call");
        assert_eq!(cat(member("items", "append")), "native_python");
        assert_eq!(cat(member("service", "run")), "external_call");
    }
}
