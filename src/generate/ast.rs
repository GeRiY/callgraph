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
            return Err(GenError::new(crate::t!("empty type", "üres típus")));
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

    /// Minden `Named` név átírása (a típusfában mindenhol).
    pub fn rename_named(&mut self, f: &mut dyn FnMut(&str) -> String) {
        match self {
            Self::Named(n) => *n = f(n),
            Self::List(t) | Self::Opt(t) | Self::Iter(t) => t.rename_named(f),
            Self::Map(k, v) => {
                k.rename_named(f);
                v.rename_named(f);
            }
            Self::Union(items) => items.iter_mut().for_each(|t| t.rename_named(f)),
            _ => {}
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
    /// Trait-használat a törzs elején (`use A, B;`): `c:Post > use:HasFactory,Notifiable`.
    pub traits: Vec<TypeRef>,
    /// Névtelen osztály (`a:<Migration>`): `return new class extends Migration { … };`.
    pub anonymous: bool,
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
            traits: Vec::new(),
            anonymous: false,
        }
    }
}

/// Explicit import (`use:Foo\Bar as Q`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseImport {
    pub fqcn: String,
    pub alias: Option<String>,
}

/// Nyers szövegblokk (`php{ … }` / `raw{ … }`): utasítások osztály nélkül (route-ok, `bootstrap/app.php`,
/// Blade-nézet). `php`: a PHP-fájl `<?php` fejlécet kap és importokat; egyébként a szöveg szó szerint kerül be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlock {
    pub text: String,
    pub php: bool,
}

impl Program {
    /// A minősített típusnevek (`\Illuminate\Http\Response` vagy `Illuminate\Http\Response`) importtá alakítása: a
    /// típus rövid nevet kap, a teljes név pontosan egy `use` sor lesz. Ha a rövid név már másra foglalt (más import
    /// vagy a most létrehozott típus neve), a típus `\Teljes\Név` marad, import nélkül. Az egyszegmensű `\Foo` változatlan.
    pub fn import_qualified_types(&mut self) {
        let mut taken: Vec<(String, String)> = self
            .uses
            .iter()
            .map(|u| {
                let short = u.fqcn.rsplit('\\').next().unwrap_or(&u.fqcn).to_owned();
                (u.alias.clone().unwrap_or(short), u.fqcn.clone())
            })
            .collect();
        let own: Vec<String> = self.decls.iter().map(|d| d.name.clone()).collect();
        let mut added: Vec<UseImport> = Vec::new();
        let mut rewrite = |name: &str| -> String {
            let full = name.trim_start_matches('\\');
            let Some((_, short)) = full.rsplit_once('\\') else {
                return name.to_owned();
            };
            if own.iter().any(|o| o == short) {
                return format!("\\{full}");
            }
            match taken.iter().find(|(alias, _)| alias == short) {
                Some((_, fqcn)) if fqcn == full => short.to_owned(),
                Some(_) => format!("\\{full}"),
                None => {
                    taken.push((short.to_owned(), full.to_owned()));
                    added.push(UseImport {
                        fqcn: full.to_owned(),
                        alias: None,
                    });
                    short.to_owned()
                }
            }
        };
        let mut types: Vec<&mut TypeRef> = Vec::new();
        for decl in &mut self.decls {
            types.extend(decl.extends.iter_mut());
            types.extend(decl.implements.iter_mut());
            types.extend(decl.backing.iter_mut());
            types.extend(decl.traits.iter_mut());
            for member in &mut decl.members {
                types.extend(member.ty.iter_mut());
                types.extend(member.params.iter_mut().filter_map(|p| p.ty.as_mut()));
            }
        }
        for member in &mut self.loose {
            types.extend(member.ty.iter_mut());
            types.extend(member.params.iter_mut().filter_map(|p| p.ty.as_mut()));
        }
        for ty in types {
            ty.rename_named(&mut rewrite);
        }
        self.uses.extend(added);
    }
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
    /// Nyers blokk (`php{…}` / `raw{…}`), típus és csupasz tag nélkül.
    pub raw: Option<RawBlock>,
}
