//! A Rust hívás-kategorizálás szabályai (sorrendben; az első illeszkedő dönt).
//!
//! | Szabály | Kategória |
//! |---|---|
//! | `MacroRule` | `macro_use` (minden makróhívás; `resolved` = az import szerinti teljes út) |
//! | `BoundLocalRule` | `external_call` (helyi változó / paraméter / closure hívása) |
//! | `SelfMethodRule` | `internal_helper` (`self.x()` a container saját metódusára) |
//! | `LocalPathRule` | `internal_helper` (`Self::x`, a fájl saját típusa / függvénye) |
//! | `LocalMethodRule` | `internal_helper` (a metódusnév egyetlen saját containerben létezik) |
//! | `NativeRule` | `native_rust` (std / core / alloc, prelude, gyakori std-metódus) |
//! | `FrameworkRule` | `framework_use` (ismert framework crate) |
//! | `ModuleRule` | `module_use` (a crate saját eleme, vagy feloldhatatlan név) |
//! | `PackageRule` | `package_use` (más crate) |
//! | tartalék | `external_call` (ismeretlen fogadójú metódus) |

use super::data::{STD_METHODS, STD_NAMES, STD_ROOTS};
use super::{CallInput, CallRule, Callee, Verdict};
use crate::core::Rule;
use crate::lang::rs::framework::is_framework_crate;
use crate::lang::rs::resolve::{Resolution, crate_key};

pub(super) fn default_rules() -> Vec<Box<CallRule>> {
    vec![
        Box::new(MacroRule),
        Box::new(BoundLocalRule),
        Box::new(SelfMethodRule),
        Box::new(LocalPathRule),
        Box::new(LocalMethodRule),
        Box::new(NativeRule),
        Box::new(FrameworkRule),
        Box::new(ModuleRule),
        Box::new(PackageRule),
    ]
}

fn resolution(input: &CallInput<'_>) -> Option<Resolution> {
    match input.callee {
        Callee::Path(segs) | Callee::Macro(segs) => {
            Some(input.resolver.resolve(segs, input.container, input.inline))
        }
        Callee::Method { .. } => None,
    }
}

fn is_std(path: &[String]) -> bool {
    path.first().is_some_and(|root| {
        STD_ROOTS.contains(&root.as_str()) || STD_NAMES.contains(&root.as_str())
    })
}

/// Makróhívás: saját kategória; a `resolved` az import szerinti teljes út (`tracing::info!`).
struct MacroRule;
impl Rule<CallInput<'_>> for MacroRule {
    type Output = Verdict;
    fn apply(&self, input: &CallInput<'_>) -> Option<Verdict> {
        let Callee::Macro(_) = input.callee else {
            return None;
        };
        let resolved = match resolution(input)? {
            Resolution::External(path) | Resolution::Crate(path) => {
                Some(format!("{}!", path.join("::")))
            }
            _ => None,
        };
        Some(Verdict::new("macro_use", resolved))
    }
}

/// Egyszegmensű hívás egy helyi változóra / paraméterre (closure, függvény-mutató).
struct BoundLocalRule;
impl Rule<CallInput<'_>> for BoundLocalRule {
    type Output = Verdict;
    fn apply(&self, input: &CallInput<'_>) -> Option<Verdict> {
        matches!(input.callee, Callee::Path(segs) if segs.len() == 1 && input.bound_locally)
            .then(|| Verdict::new("external_call", None))
    }
}

/// `self.x()`, ha `x` a hívó container saját metódusa.
struct SelfMethodRule;
impl Rule<CallInput<'_>> for SelfMethodRule {
    type Output = Verdict;
    fn apply(&self, input: &CallInput<'_>) -> Option<Verdict> {
        let Callee::Method {
            name,
            on_self: true,
        } = input.callee
        else {
            return None;
        };
        input.resolver.has_method(input.container, name).then(|| {
            Verdict::new(
                "internal_helper",
                Some(format!("{}::{name}", input.container)),
            )
        })
    }
}

/// A fájlban definiált elem (`Self::new`, `Típus::f`, `segéd()`, enum-variáns).
struct LocalPathRule;
impl Rule<CallInput<'_>> for LocalPathRule {
    type Output = Verdict;
    fn apply(&self, input: &CallInput<'_>) -> Option<Verdict> {
        match resolution(input)? {
            Resolution::Local(key) => Some(Verdict::new("internal_helper", Some(key))),
            _ => None,
        }
    }
}

/// Metódushívás, amelynek neve pontosan egy saját containerben létezik (`self`-es metódusként).
struct LocalMethodRule;
impl Rule<CallInput<'_>> for LocalMethodRule {
    type Output = Verdict;
    fn apply(&self, input: &CallInput<'_>) -> Option<Verdict> {
        let Callee::Method { name, .. } = input.callee else {
            return None;
        };
        let container = input.resolver.unique_method(name)?;
        Some(Verdict::new(
            "internal_helper",
            Some(format!("{container}::{name}")),
        ))
    }
}

/// A standard könyvtár: std / core / alloc utak, prelude- és gyakori std-nevek, std-metódusok.
struct NativeRule;
impl Rule<CallInput<'_>> for NativeRule {
    type Output = Verdict;
    fn apply(&self, input: &CallInput<'_>) -> Option<Verdict> {
        match input.callee {
            Callee::Method { name, .. } => STD_METHODS
                .contains(&name.as_str())
                .then(|| Verdict::new("native_rust", None)),
            _ => match resolution(input)? {
                Resolution::External(path) | Resolution::Unknown(path) if is_std(&path) => {
                    Some(Verdict::new("native_rust", Some(path.join("::"))))
                }
                _ => None,
            },
        }
    }
}

/// Ismert framework crate (axum, actix-web, tokio, serde…).
struct FrameworkRule;
impl Rule<CallInput<'_>> for FrameworkRule {
    type Output = Verdict;
    fn apply(&self, input: &CallInput<'_>) -> Option<Verdict> {
        match resolution(input)? {
            Resolution::External(path) if is_framework_crate(&path[0]) => {
                Some(Verdict::new("framework_use", Some(path.join("::"))))
            }
            _ => None,
        }
    }
}

/// A crate saját eleme (`crate::` / `super::` / `self::` vagy ilyen import), illetve feloldhatatlan
/// név (glob-import vagy a szülő-modul eleme). A `resolved` a rövid kulcs (`args::parse`, `Method::new`).
struct ModuleRule;
impl Rule<CallInput<'_>> for ModuleRule {
    type Output = Verdict;
    fn apply(&self, input: &CallInput<'_>) -> Option<Verdict> {
        match resolution(input)? {
            Resolution::Crate(path) => Some(Verdict::new("module_use", Some(crate_key(&path)))),
            Resolution::Unknown(_) => Some(Verdict::new("module_use", None)),
            _ => None,
        }
    }
}

/// Más crate (külső függőség), a teljes úttal.
struct PackageRule;
impl Rule<CallInput<'_>> for PackageRule {
    type Output = Verdict;
    fn apply(&self, input: &CallInput<'_>) -> Option<Verdict> {
        match resolution(input)? {
            Resolution::External(path) => Some(Verdict::new("package_use", Some(path.join("::")))),
            _ => None,
        }
    }
}
