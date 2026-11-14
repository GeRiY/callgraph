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
    /// A tag által bevezetett név (a composable `return { … }`-jához); a props / emits / horog nem ad nevet.
    pub fn declared_name(&self) -> Option<&str> {
        match self {
            Self::Ref { name, .. }
            | Self::Computed { name, .. }
            | Self::Func { name, .. }
            | Self::Form { name, .. }
            | Self::Const { name, .. } => Some(name),
            Self::Props(_)
            | Self::Emits(_)
            | Self::Hook { .. }
            | Self::Type { .. }
            | Self::Raw(_)
            | Self::Destructure { .. } => None,
        }
    }

    /// Minden név, amit a tag a fájlban deklarál (ütközés-vizsgálathoz): a destructuring nevei és a típus neve is;
    /// a composable `return`-jébe csak a `declared_name` kerül.
    pub fn declares(&self) -> Vec<&str> {
        match self {
            Self::Destructure { names, .. } => names.iter().map(String::as_str).collect(),
            Self::Type { name, .. } => vec![name.as_str()],
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
    /// Hány szakasz van benne (script / template / style).
    pub fn sections(&self) -> usize {
        usize::from(self.script.is_some())
            + usize::from(self.template.is_some())
            + usize::from(self.style.is_some())
    }
}
