//! Nyelvfüggetlen AST: típus-deklaráció (`class | interface | trait | enum | struct`), tag (`method | field | const
//! | ctor`), paraméter és `TypeRef`. A PHP-specifikus részek (promoted ctor-paraméter, `readonly`, `strict_types`,
//! `use`, `$`) vagy az emitterben / builderben élnek, vagy opcionális, általános attribútumként szerepelnek itt.
//! A metódus-törzs nyers szöveg, nyelvenként.

use super::error::{GenError, GenResult};

/// A típus-deklaráció fajtája.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Class,
    Interface,
    Trait,
    Enum,
    Struct,
}

impl TypeKind {
    /// A `FileGraph` container-fajtájából (`class` / `interface` / `trait` / `enum`); ismeretlen → osztály.
    pub fn from_graph(kind: &str) -> Self {
        match kind {
            "interface" => Self::Interface,
            "trait" => Self::Trait,
            "enum" => Self::Enum,
            _ => Self::Class,
        }
    }
}

/// Láthatóság.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

/// A nyelvfüggetlen típus-szókincs: `int float bool str bytes void mixed`, `list<T>`, `map<K,V>`, `opt<T>` / `?T`,
/// `iter<T>`, uniók (`A|B`) és a felhasználói / natív nevek úgy, ahogy le lettek írva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Int,
    Float,
    Bool,
    Str,
    Bytes,
    Void,
    Mixed,
    List(Box<TypeRef>),
    Map(Box<TypeRef>, Box<TypeRef>),
    Opt(Box<TypeRef>),
    Iter(Box<TypeRef>),
    Union(Vec<TypeRef>),
    /// Felhasználói típus vagy a célnyelv natív neve (`Order`, `string`, `array`, `self`, `\Foo\Bar` …).
    Named(String),
}

impl TypeRef {
    /// Típus-kifejezés értelmezése (`list<Order>`, `?int`, `int|string`, `map<str,list<int>>`, `Order`).
    pub fn parse(text: &str) -> GenResult<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Err(GenError::new(crate::t!(
                "empty type",
                "üres típus"
            )));
        }
        if let Some(inner) = text.strip_prefix('?') {
            return Ok(Self::Opt(Box::new(Self::parse(inner)?)));
        }
        let parts = super::head::split_top(text, '|');
        if parts.len() > 1 {
            let members = parts
                .iter()
                .map(|p| Self::parse(p))
                .collect::<GenResult<Vec<_>>>()?;
            return Ok(Self::Union(members));
        }
        if let Some(open) = text.find('<') {
            return Self::parse_generic(text, open);
        }
        Ok(match text {
            "int" => Self::Int,
            "float" => Self::Float,
            "bool" => Self::Bool,
            "str" => Self::Str,
            "bytes" => Self::Bytes,
            "void" => Self::Void,
            "mixed" => Self::Mixed,
            other => {
                if !other
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '\\')
                {
                    return Err(GenError::new(crate::tf!(
                        "bad type: {}",
                        "hibás típus: {}",
                        other
                    )));
                }
                Self::Named(other.to_owned())
            }
        })
    }

    fn parse_generic(text: &str, open: usize) -> GenResult<Self> {
        let name = text[..open].trim();
        let inner = text[open + 1..]
            .strip_suffix('>')
            .ok_or_else(|| GenError::new(crate::tf!("bad type: {}", "hibás típus: {}", text)))?;
        let args = super::head::split_top(inner, ',')
            .iter()
            .map(|a| Self::parse(a))
            .collect::<GenResult<Vec<_>>>()?;
        let wrong = || {
            GenError::new(crate::tf!(
                "bad type: {} (use list<T>, map<K,V>, opt<T>, iter<T>)",
                "hibás típus: {} (list<T>, map<K,V>, opt<T>, iter<T> használható)",
                text
            ))
        };
        let mut it = args.into_iter();
        match (name, it.next(), it.next(), it.next()) {
            ("list", Some(t), None, _) => Ok(Self::List(Box::new(t))),
            ("map", Some(k), Some(v), None) => Ok(Self::Map(Box::new(k), Box::new(v))),
            ("opt", Some(t), None, _) => Ok(Self::Opt(Box::new(t))),
            ("iter", Some(t), None, _) => Ok(Self::Iter(Box::new(t))),
            _ => Err(wrong()),
        }
    }

    /// Minden felhasználói / natív név a típusban (importálandó jelöltek a hívó szűrése után).
    pub fn names(&self, out: &mut Vec<String>) {
        match self {
            Self::Named(n) => out.push(n.clone()),
            Self::List(t) | Self::Opt(t) | Self::Iter(t) => t.names(out),
            Self::Map(k, v) => {
                k.names(out);
                v.names(out);
            }
            Self::Union(items) => items.iter().for_each(|t| t.names(out)),
            Self::Int
            | Self::Float
            | Self::Bool
            | Self::Str
            | Self::Bytes
            | Self::Void
            | Self::Mixed => {}
        }
    }
}

/// Függvény- / konstruktor-paraméter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub default: Option<String>,
    pub variadic: bool,
    pub by_ref: bool,
    /// Láthatóság-előtag: a konstruktorban promoted property (PHP).
    pub promote: Option<Visibility>,
    pub readonly: bool,
}

/// A tag fajtája.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Method,
    Field,
    Const,
    Ctor,
}

/// Egy típus tagja.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub kind: MemberKind,
    pub name: String,
    pub visibility: Option<Visibility>,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly: bool,
    pub params: Vec<Param>,
    /// Mező / konstans típusa, metódus visszatérési típusa.
    pub ty: Option<TypeRef>,
    /// Mező / konstans alapértéke (nyers szöveg).
    pub default: Option<String>,
    /// Nyers törzs; `None` = nincs megadva (az emitter szabálya szerint kap vázat), `Some("")` = üres törzs.
    pub body: Option<String>,
}

impl Member {
    pub fn new(kind: MemberKind, name: &str) -> Self {
        Self {
            kind,
            name: name.to_owned(),
            visibility: None,
            is_static: false,
            is_abstract: false,
            is_final: false,
            is_readonly: false,
            params: Vec::new(),
            ty: None,
            default: None,
            body: None,
        }
    }
}

/// Enum-eset (`Draft=draft`; tiszta enumnál érték nélkül).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumCase {
    pub name: String,
    pub value: Option<String>,
}

/// Típus-deklaráció.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDecl {
    pub kind: TypeKind,
    pub name: String,
    pub is_final: bool,
    pub is_abstract: bool,
    pub is_readonly: bool,
    pub extends: Vec<TypeRef>,
    pub implements: Vec<TypeRef>,
    /// Enum háttér-típusa (`e:Status:string(...)`).
    pub backing: Option<TypeRef>,
    pub cases: Vec<EnumCase>,
    pub members: Vec<Member>,
}

impl TypeDecl {
    pub fn new(kind: TypeKind, name: &str) -> Self {
        Self {
            kind,
            name: name.to_owned(),
            is_final: false,
            is_abstract: false,
            is_readonly: false,
            extends: Vec::new(),
            implements: Vec::new(),
            backing: None,
            cases: Vec::new(),
            members: Vec::new(),
        }
    }
}

/// Explicit import (`use:Foo\Bar as Q`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseImport {
    pub fqcn: String,
    pub alias: Option<String>,
}

/// Egy kifejezés eredménye.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    /// `ns:` felülírás.
    pub namespace: Option<String>,
    pub uses: Vec<UseImport>,
    pub decls: Vec<TypeDecl>,
    /// Típus nélküli, „csupasz” tagok (`--into` / `--replace` célra).
    pub loose: Vec<Member>,
}
