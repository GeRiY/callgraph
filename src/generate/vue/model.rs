//! A Vue-generátor modellje: a script-tagok, a template-elemek, a stílus-szabályok és a kifejezés egésze
//! (`Program`). A típusok a közös `TypeRef`-et használják (`int`, `list<T>`, `opt<T>` …).

use crate::generate::ast::TypeRef;

/// Egy `props(...)` bejegyzés: `title:str`, `size?:int=3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prop {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub required: bool,
    pub default: Option<String>,
}

/// Egy `emits(...)` bejegyzés: `save:int` (payload) vagy `close`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emit {
    pub name: String,
    pub payload: Option<TypeRef>,
}

/// Függvény-paraméter: `e:Event`, `n?:int`, `size:int=3`, `...rest:list<int>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VParam {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub default: Option<String>,
    pub optional: bool,
    pub rest: bool,
}

/// A `form(...)` egy mezője: név, (típus,) alapérték.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub default: Option<String>,
}

/// Egy `type:Név{…}` / inline objektum-típus mezője: `flag:str`, `on?:bool`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeField {
    pub name: String,
    pub optional: bool,
    pub ty: TypeRef,
}

/// Egy modul-szintű tag exportja: `-fn:x` nem exportált, `!fn:x` `export default`, egyébként nevesített export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Export {
    Named,
    None,
    Default,
}

/// Osztálytag láthatósága: `+` public, `-` private, `#` protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vis {
    Public,
    Private,
    Protected,
}

/// Konstruktor-paraméter: a láthatóság-előtagos (`-dep:Repo`, `+n:int=3`) TS-ben paraméter-property, JS-ben
/// `this.x = x` értékadás.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtorParam {
    pub param: VParam,
    pub vis: Option<Vis>,
    pub readonly: bool,
}

/// Az osztálytag fajtája.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassKind {
    Ctor {
        params: Vec<CtorParam>,
        body: Option<String>,
    },
    Field {
        name: String,
        ty: Option<TypeRef>,
        init: Option<String>,
    },
    Method {
        name: String,
        params: Vec<VParam>,
        ret: Option<TypeRef>,
        body: Option<String>,
        is_async: bool,
    },
}

/// Egy osztálytag: `+name(a:int):R{…}`, `-$secret:str`, `%$id:str`, `::make()`, `~run()`, `ctor(-dep:Repo)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassMember {
    pub vis: Vis,
    pub is_static: bool,
    pub is_abstract: bool,
    pub readonly: bool,
    pub kind: ClassKind,
}

impl ClassMember {
    /// A tag neve (a konstruktoré `constructor`).
    pub fn name(&self) -> &str {
        match &self.kind {
            ClassKind::Ctor { .. } => "constructor",
            ClassKind::Field { name, .. } | ClassKind::Method { name, .. } => name,
        }
    }
}

/// `class:Név<Ős>[A,B]>…`: osztály-deklaráció.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDecl {
    pub name: String,
    pub is_abstract: bool,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub members: Vec<ClassMember>,
}

/// A script egy tagja.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    Props(Vec<Prop>),
    Emits(Vec<Emit>),
    Ref {
        name: String,
        ty: Option<TypeRef>,
        init: Option<String>,
    },
    Computed {
        name: String,
        ty: Option<TypeRef>,
        body: Option<String>,
    },
    Func {
        name: String,
        params: Vec<VParam>,
        ret: Option<TypeRef>,
        body: Option<String>,
        is_async: bool,
    },
    Form {
        name: String,
        fields: Vec<FormField>,
    },
    Const {
        name: String,
        ty: Option<TypeRef>,
        value: Option<String>,
        mutable: bool,
    },
    /// `type:Név{flag:str,on?:bool}` → TS `interface`, JS-ben JSDoc `@typedef`.
    Type {
        name: String,
        fields: Vec<TypeField>,
    },
    /// `type:Név=A|B` → TS `type Név = A | B`, JS-ben JSDoc `@typedef {A|B} Név`.
    Alias {
        name: String,
        ty: TypeRef,
    },
    /// `class:Név<Ős>[A,B]>…` (modul-szintű osztály).
    Class(Box<ClassDecl>),
    /// Modul-szintű export-módosító: `-…` (nem exportált) vagy `!…` (`export default`).
    Exported {
        export: Export,
        member: Box<Member>,
    },
    /// `raw{ utasítás(ok) }`: szó szerint kiírt sorok.
    Raw(String),
    /// `const:{ a, b }=useX()`: `names` a kötött nevek, `pattern` a kapcsos zárójelek közti szöveg.
    Destructure {
        names: Vec<String>,
        pattern: String,
        value: String,
        mutable: bool,
    },
    /// Életciklus-horog: `on:mounted{…}` → `onMounted(() => { … })`.
    Hook {
        name: String,
        body: Option<String>,
    },
}

impl Member {
    /// Csak Vue-komponensben / composable-ben értelmes tag (props, emits, ref, computed, form, `on:`).
    pub fn is_vue_only(&self) -> bool {
        match self {
            Self::Props(_)
            | Self::Emits(_)
            | Self::Ref { .. }
            | Self::Computed { .. }
            | Self::Form { .. }
            | Self::Hook { .. } => true,
            Self::Exported { member, .. } => member.is_vue_only(),
            _ => false,
        }
    }

    /// A tag által bevezetett név (a composable `return { … }`-jához); a props / emits / horog nem ad nevet.
    pub fn declared_name(&self) -> Option<&str> {
        match self {
            Self::Class(c) => Some(c.name.as_str()),
            Self::Exported { member, .. } => member.declared_name(),
            Self::Ref { name, .. }
            | Self::Computed { name, .. }
            | Self::Func { name, .. }
            | Self::Form { name, .. }
            | Self::Const { name, .. } => Some(name),
            Self::Props(_)
            | Self::Emits(_)
            | Self::Hook { .. }
            | Self::Type { .. }
            | Self::Alias { .. }
            | Self::Raw(_)
            | Self::Destructure { .. } => None,
        }
    }

    /// Minden név, amit a tag a fájlban deklarál (ütközés-vizsgálathoz): a destructuring nevei és a típus neve is;
    /// a composable `return`-jébe csak a `declared_name` kerül.
    pub fn declares(&self) -> Vec<&str> {
        match self {
            Self::Destructure { names, .. } => names.iter().map(String::as_str).collect(),
            Self::Type { name, .. } | Self::Alias { name, .. } => vec![name.as_str()],
            Self::Exported { member, .. } => member.declares(),
            other => other.declared_name().into_iter().collect(),
        }
    }
}

/// Egy import-sor: `import Default, { a, b } from 'source'`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Import {
    pub source: String,
    pub default: Option<String>,
    pub named: Vec<String>,
}

/// A script szakasz: tagok és a kifejezésben megadott (`use:`) importok.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Script {
    pub members: Vec<Member>,
    pub imports: Vec<Import>,
    /// A kiterjesztés- és kapcsos zárójel nélküli `use:<út>` importok forrásai: a generátor a célfájl alapján
    /// dönti el, hogy nevesített (`export function useX`) vagy default import kell.
    pub guess: Vec<String>,
}

/// Elem-attribútum: érték nélkül logikai (`disabled`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub value: Option<String>,
}

/// Template-elem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attrs: Vec<Attr>,
    pub text: Option<String>,
    pub children: Vec<Element>,
}

/// Stílus-szabály: szelektor és nyers deklarációs törzs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub selector: String,
    pub body: String,
}

/// A `<style>` szakasz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    pub scoped: bool,
    pub rules: Vec<Rule>,
}

/// A kifejezés fajtája.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// `vue:Név > script + template + style`.
    Sfc,
    /// `comp:useCart > tagok`.
    Composable,
    /// `react:Név > props(…) + ref:… + jsx > …`: React függvény-komponens (`.tsx` / `.jsx`).
    React,
    /// `hook:useCart > tagok`: React custom hook (`.ts` / `.js`).
    Hook,
    /// `mod:név > tagok`: általános JS / TS modul (exportált függvények, konstansok, típusok, osztályok).
    Module,
    /// Fej nélküli szakaszok (`script>…`, `template>…`) vagy csupasz tagok (`--into` célra).
    #[default]
    Parts,
}

/// Egy kifejezés eredménye.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    pub kind: Kind,
    pub name: Option<String>,
    pub script: Option<Script>,
    pub template: Option<Vec<Element>>,
    pub style: Option<Style>,
}

impl Program {
    /// Modul-szintű (osztály / export-módosítós) tag van-e a csupasz tagok között, vagy `mod:` a fej.
    pub fn module_like(&self) -> bool {
        self.kind == Kind::Module
            || (self.kind == Kind::Parts
                && self.script.as_ref().is_some_and(|s| {
                    s.members
                        .iter()
                        .any(|m| matches!(m, Member::Class(_) | Member::Exported { .. }))
                }))
    }

    /// Modulként írandó-e: `mod:` fej, vagy csupasz tagok Vue-specifikus tag (ref / computed / props …) nélkül.
    pub fn is_module(&self) -> bool {
        match self.kind {
            Kind::Module => true,
            Kind::Parts => {
                self.template.is_none()
                    && self.style.is_none()
                    && self.script.as_ref().is_some_and(|s| {
                        !s.members.is_empty() && !s.members.iter().any(Member::is_vue_only)
                    })
            }
            _ => false,
        }
    }

    /// Hány szakasz van benne (script / template / style).
    pub fn sections(&self) -> usize {
        usize::from(self.script.is_some())
            + usize::from(self.template.is_some())
            + usize::from(self.style.is_some())
    }
}
