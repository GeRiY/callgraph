//! A fájl elemeinek közös előfeldolgozása: containerek, függvények, konstansok, mezők és felső szintű
//! makrók a végleges (minősített, ütközésmentes) nevekkel. Minden visitor ebből dolgozik, így a nevek
//! (container, caller) mindenhol azonosak.

mod impls;
mod walk;

use syn::{Attribute, Block, Field, ItemMacro, Macro, Signature, Visibility};

pub(crate) use walk::collect;

/// Egy container (`module`, `mod`, `struct`, `enum`, `union`, `trait`, `impl`).
pub(crate) struct ContainerScope<'a> {
    pub kind: &'static str,
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub attrs: &'a [Attribute],
    /// `trait A: B + C` → `["B", "C"]`.
    pub extends: Vec<String>,
    /// `impl Trait for X` → X `implements`-e.
    pub implements: Vec<String>,
}

/// A függvény helye: szabad függvény, `impl`-blokk (trait-implementációnál a trait nevével) vagy trait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FnRole {
    Free,
    Impl(Option<String>),
    Trait,
}

/// Egy függvény / metódus.
pub(crate) struct FnScope<'a> {
    pub container: String,
    pub name: String,
    pub inline: Vec<String>,
    pub role: FnRole,
    pub attrs: &'a [Attribute],
    pub vis: Option<&'a Visibility>,
    pub sig: &'a Signature,
    pub block: Option<&'a Block>,
    pub start: u32,
    pub end: u32,
}

impl FnScope<'_> {
    /// Van-e `self` fogadója.
    pub fn has_receiver(&self) -> bool {
        self.sig.receiver().is_some()
    }
}

/// Konstans-jellegű elem.
pub(crate) enum ConstNode<'a> {
    Const(&'a syn::ItemConst),
    Static(&'a syn::ItemStatic),
    Type(&'a syn::ItemType),
    ImplConst(&'a syn::ImplItemConst),
    TraitConst(&'a syn::TraitItemConst),
    Variant(&'a syn::Variant),
}

/// Egy konstans a containerével.
pub(crate) struct ConstScope<'a> {
    pub container: String,
    pub inline: Vec<String>,
    pub node: ConstNode<'a>,
    pub line: u32,
}

/// Egy struct- / union-mező.
pub(crate) struct FieldScope<'a> {
    pub container: String,
    pub field: &'a Field,
    pub index: usize,
}

/// Felső szintű makró: `macro_rules!` definíció vagy hívás (`thread_local! { … }`).
pub(crate) enum MacroScope<'a> {
    Def {
        container: String,
        item: &'a ItemMacro,
        name: String,
    },
    Call {
        container: String,
        inline: Vec<String>,
        mac: &'a Macro,
    },
}

/// A fájl összes eleme.
#[derive(Default)]
pub(crate) struct Scopes<'a> {
    pub containers: Vec<ContainerScope<'a>>,
    pub fns: Vec<FnScope<'a>>,
    pub consts: Vec<ConstScope<'a>>,
    pub fields: Vec<FieldScope<'a>>,
    pub macros: Vec<MacroScope<'a>>,
}
