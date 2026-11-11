//! Go hívás-kategorizálás (Chain of Responsibility a `core::Chain`-en): parser-független `CallInfo`-ból.

use std::collections::{HashMap, HashSet};

use super::known::{BUILTINS, NATIVE_METHODS, STDLIB_ROOTS, is_in};
use crate::core::{Chain, Rule};

/// Egy hívás leírása a kategorizáláshoz.
#[derive(Debug, Clone, Default)]
pub(crate) struct CallInfo {
    /// A hívott név (az utolsó tag): `Println`, `Find`, `make`.
    pub name: String,
    /// Tag-hívás (`a.b()`), nem puszta azonosító.
    pub member: bool,
    /// A lánc bal szélső azonosítója (`http.Get()` → `http`; `r.repo.Find()` → `r`).
    pub root: Option<String>,
    /// A fogadó pontosan a metódus receiver-neve (`r.Find()`).
    pub on_self: bool,
    /// A fogadó maga is hívás (`a().b()`).
    pub chained: bool,
}

/// A fájl ismert nevei: csomag-szintű függvények, típusok, metódusnevek és az importált csomagnevek.
#[derive(Debug, Default)]
pub(crate) struct FileSymbols {
    /// Csomag-szintű függvények.
    pub functions: HashSet<String>,
    /// Típusok (struct, interface, alias, definiált típus).
    pub types: HashSet<String>,
    /// A fájlban definiált metódusnevek (receiveres metódusok és interfész-metódusok).
    pub methods: HashSet<String>,
    /// Importált csomagnév (alias vagy az út utolsó eleme) → teljes import-út.
    pub imports: HashMap<String, String>,
}

/// A kategorizáló bemenete: a hívás és a fájl szimbólumai.
pub(crate) struct CategorizeInput<'a> {
    pub call: &'a CallInfo,
    pub symbols: &'a FileSymbols,
}

type GoRule = dyn for<'a> Rule<CategorizeInput<'a>, Output = &'static str> + Send + Sync;

/// A hívás-kategorizáló lánc (az első illeszkedő szabály dönt; tartalék: `external_call`).
pub(crate) struct Categorizer {
    chain: Chain<GoRule, &'static str>,
}

impl Default for Categorizer {
    fn default() -> Self {
        Self {
            chain: Chain::new(
                vec![
                    Box::new(OnReceiver),
                    Box::new(Bare),
                    Box::new(TypeMember),
                    Box::new(PackageMember),
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

/// Az import-út a standard könyvtáré-e (az első szegmense ismert stdlib-gyökér).
pub(crate) fn is_stdlib(path: &str) -> bool {
    is_in(STDLIB_ROOTS, path.split('/').next().unwrap_or(path))
}

/// `r.Method()` a receiveren: az ugyanazon típus (vagy beágyazott őse) metódusa.
struct OnReceiver;
impl Rule<CategorizeInput<'_>> for OnReceiver {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        c.on_self.then(|| {
            if i.symbols.methods.contains(&c.name) {
                "internal_helper"
            } else {
                "external_call"
            }
        })
    }
}

/// Puszta hívás: fájlbeli típus (konverzió) / függvény, beépített, ismeretlen (más fájl, ugyanabból a csomagból).
struct Bare;
impl Rule<CategorizeInput<'_>> for Bare {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let c = i.call;
        if c.member {
            return None;
        }
        let s = i.symbols;
        Some(if s.types.contains(&c.name) {
            "instantiation"
        } else if s.functions.contains(&c.name) {
            "internal_helper"
        } else if is_in(BUILTINS, &c.name) {
            "native_go"
        } else {
            "global_helper"
        })
    }
}

/// `Típus.Metódus()` a fájlban definiált típuson (method expression): statikus hívás.
struct TypeMember;
impl Rule<CategorizeInput<'_>> for TypeMember {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let root = i.call.root.as_deref()?;
        (i.symbols.types.contains(root) && !i.call.chained).then_some("static_call")
    }
}

/// `csomag.Függvény()` importált csomagon: stdlib → `native_go`, más → `external_call`.
struct PackageMember;
impl Rule<CategorizeInput<'_>> for PackageMember {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        let root = i.call.root.as_deref()?;
        let path = i.symbols.imports.get(root)?;
        if i.call.chained {
            return None;
        }
        Some(if is_stdlib(path) {
            "native_go"
        } else {
            "external_call"
        })
    }
}

/// Standard típus gyakori metódusa (`Error`, `Lock`, `Close`…) bármilyen fogadón.
struct NativeMethod;
impl Rule<CategorizeInput<'_>> for NativeMethod {
    type Output = &'static str;
    fn apply(&self, i: &CategorizeInput<'_>) -> Option<&'static str> {
        is_in(NATIVE_METHODS, &i.call.name).then_some("native_go")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols() -> FileSymbols {
        let mut s = FileSymbols::default();
        s.functions.insert("helper".into());
        s.types.insert("Repo".into());
        s.methods.insert("Save".into());
        s.imports.insert("fmt".into(), "fmt".into());
        s.imports.insert("http".into(), "net/http".into());
        s.imports
            .insert("gin".into(), "github.com/gin-gonic/gin".into());
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
            on_self: root == "r",
            ..CallInfo::default()
        }
    }

    #[test]
    fn bare_calls() {
        assert_eq!(cat(bare("helper")), "internal_helper");
        assert_eq!(cat(bare("Repo")), "instantiation");
        assert_eq!(cat(bare("make")), "native_go");
        assert_eq!(cat(bare("len")), "native_go");
        assert_eq!(cat(bare("string")), "native_go");
        assert_eq!(cat(bare("mystery")), "global_helper");
    }

    #[test]
    fn members() {
        assert_eq!(cat(member("r", "Save")), "internal_helper");
        assert_eq!(cat(member("r", "Other")), "external_call");
        assert_eq!(cat(member("Repo", "Save")), "static_call");
        assert_eq!(cat(member("fmt", "Println")), "native_go");
        assert_eq!(cat(member("http", "Get")), "native_go");
        assert_eq!(cat(member("gin", "Default")), "external_call");
        assert_eq!(cat(member("mu", "Lock")), "native_go");
        assert_eq!(cat(member("service", "Run")), "external_call");
    }

    #[test]
    fn stdlib_detection() {
        assert!(is_stdlib("net/http") && is_stdlib("fmt") && is_stdlib("encoding/json"));
        assert!(!is_stdlib("github.com/a/b") && !is_stdlib("myapp/internal/db"));
    }
}
