//! Kapcsolatok, amelyek nem puszta hívások: a fogadó típusának feloldása a deklarációból, metódus-hivatkozás
//! (`Composer(onSend: vm.send)`), closure-bekötés (`socket.onFrame = { … }`), `.environment(store)` befecskendezés
//! és a navigációs célpontok (`navigationDestination`, `sheet`).

use tree_sitter::Node;

use super::super::categorize::CallInfo;
use super::super::known::{NATIVE_TYPES, SWIFTDATA_TYPES, SWIFTUI_VIEWS, is_in};
use super::super::parser::{end_line, line, named_children};
use super::{Scope, SwiftVisitor, scan};
use crate::model::{Call, FileGraphBuilder};

/// A navigációs hívások: a zárójelükben épített nézet / ViewModel navigációs él.
const NAV_CALLS: &[&str] = &[
    "navigationDestination",
    "sheet",
    "fullScreenCover",
    "popover",
    "inspector",
    "NavigationLink",
];

/// Függvény-hivatkozást váró, címke nélküli argumentumú hívások (`items.map(vm.format)`).
const HIGHER_ORDER: &[&str] = &[
    "map",
    "compactMap",
    "flatMap",
    "forEach",
    "sink",
    "filter",
    "sorted",
    "reduce",
];

/// Visszahívást jelző címke / név: `onSend`, `action`, `completion`, `submitHandler`, `operation`…
fn callback_like(name: &str) -> bool {
    let on_prefixed = name
        .strip_prefix("on")
        .and_then(|rest| rest.chars().next())
        .is_some_and(char::is_uppercase);
    on_prefixed
        || matches!(
            name,
            "action"
                | "handler"
                | "completion"
                | "callback"
                | "perform"
                | "receiveValue"
                | "receiveCompletion"
                | "operation"
                | "execute"
                | "block"
                | "work"
        )
        || ["Handler", "Action", "Callback", "Completion"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

/// A hívás címke nélküli argumentuma visszahívás-e: `callback_like` név vagy magasabb rendű függvény
/// (`items.map(vm.format)`); az `onReceive(vm.publisher)` első argumentuma érték, nem visszahívás.
fn callee_takes_callback(callee: &str) -> bool {
    (callback_like(callee) && callee != "onReceive") || HIGHER_ORDER.contains(&callee)
}

/// Mennyire biztos, hogy egy argumentum visszahívás: címke (`action:`), a hívó neve (`.map(…)`), vagy semmi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hint {
    None,
    Callee,
    Label,
}

/// Egy fogadó-út feloldásának eredménye.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Receiver {
    /// A teljes út típusa ismert (keretrendszer-típus is lehet: `ModelContext`).
    Typed(String),
    /// Az út egy másik fájlban deklarált típusnál megakadt: `ty` az utolsó ismert elem típusa, `rest` a
    /// hátralévő property-nevek (`self.store.session.user` → `Store` + `session`, `user`).
    Pending { ty: String, rest: Vec<String> },
}

/// A stdlib / Foundation / SwiftData típus: nem projekt-szimbólum.
fn is_framework_type(ty: &str) -> bool {
    is_in(NATIVE_TYPES, ty) || is_in(SWIFTDATA_TYPES, ty)
}

impl SwiftVisitor<'_, '_> {
    /// A saját típus neve a hatókörben (`Külső.Belső`, extension-jel nélkül); fájl-szinten `None`.
    pub(super) fn own_type(&self, scope: &Scope) -> Option<String> {
        let container = scope.container.as_deref()?;
        if self.module.as_deref() == Some(container) {
            return None;
        }
        Some(container.split('@').next().unwrap_or(container).to_string())
    }

    /// A saját típus property-jének típusa (üres, ha ismeretlen); `None`, ha nincs ilyen property, vagy
    /// helyi változó / paraméter árnyékolja.
    pub(super) fn own_property(&self, scope: &Scope, name: &str) -> Option<&String> {
        if scope.locals.contains_key(name) {
            return None;
        }
        let owner = self.own_type(scope)?;
        self.symbols.props.get(scan::type_key(&owner))?.get(name)
    }

    /// Egy `a.b.c` fogadó-út típusa: `self` a saját típus, az első elem helyi változó / paraméter / property
    /// vagy (nagybetűvel) egy típus neve; a továbbiak a fájlban deklarált típusok property-jei.
    pub(super) fn receiver_type(&self, path: &str, scope: &Scope) -> Option<String> {
        match self.receiver_chain(path, scope)? {
            // a stdlib / Foundation / SwiftData fogadó nem projekt-szimbólum: nincs `Típus::név` cél
            Receiver::Typed(ty) if !is_framework_type(&ty) => Some(ty),
            _ => None,
        }
    }

    /// Egy fogadó-út feloldása: teljes típus (keretrendszer-típus is lehet), vagy — ha a lánc egy másik fájlban
    /// deklarált típusnál megakad — az utolsó ismert típus és a hátralévő property-nevek.
    pub(super) fn receiver_chain(&self, path: &str, scope: &Scope) -> Option<Receiver> {
        let mut segments = path.split('.');
        let first = segments.next()?;
        let mut ty = if matches!(first, "self" | "Self") {
            self.own_type(scope)?
        } else if let Some(local) = scope.locals.get(first) {
            local.clone()
        } else if let Some(prop) = self.own_property(scope, first) {
            prop.clone()
        } else if first.chars().next().is_some_and(char::is_uppercase)
            && !is_in(NATIVE_TYPES, first)
            && !is_in(SWIFTUI_VIEWS, first)
        {
            first.to_string()
        } else {
            return None;
        };
        let rest: Vec<&str> = segments.collect();
        for (i, segment) in rest.iter().enumerate() {
            if ty.is_empty() {
                return None;
            }
            let next = self
                .symbols
                .props
                .get(scan::type_key(&ty))
                .and_then(|props| props.get(*segment));
            match next {
                Some(next) => ty = next.clone(),
                None if is_framework_type(&ty) => return None,
                // a típus (vagy a property-je) másik fájlban deklarált: az index folytathatja a láncot
                None => {
                    return Some(Receiver::Pending {
                        ty,
                        rest: rest[i..].iter().map(|s| (*s).to_string()).collect(),
                    });
                }
            }
        }
        (!ty.is_empty()).then_some(Receiver::Typed(ty))
    }

    /// A tag-hívás feloldása `Típus::név` alakra a fogadó deklarált típusából.
    pub(super) fn resolve_member(
        &self,
        path: Option<&str>,
        name: &str,
        scope: &Scope,
    ) -> Option<String> {
        let ty = self.receiver_type(path?, scope)?;
        Some(format!("{ty}::{name}"))
    }

    /// A puszta névvel hívott tárolt closure (`onFrame?(frame)`): `Saját::név`, ha a saját típus property-je,
    /// és nem fájlban definiált függvény.
    pub(super) fn stored_closure(&self, name: &str, scope: &Scope) -> Option<String> {
        let type_like = name.chars().next().is_some_and(char::is_uppercase);
        if type_like || self.symbols.functions.contains(name) {
            return None;
        }
        self.own_property(scope, name)?;
        Some(format!("{}::{name}", self.own_type(scope)?))
    }

    /// A hívás első argumentuma (az értéke), ha nincs címkéje.
    fn first_argument<'t>(&self, call: Node<'t>) -> Option<Node<'t>> {
        let suffix = named_children(call)
            .into_iter()
            .find(|c| c.kind() == "call_suffix")?;
        let arguments = named_children(suffix)
            .into_iter()
            .find(|c| c.kind() == "value_arguments")?;
        let argument = named_children(arguments)
            .into_iter()
            .find(|c| c.kind() == "value_argument")?;
        if argument.child_by_field_name("name").is_some() {
            return None;
        }
        argument.child_by_field_name("value")
    }

    /// `.environment(store)` / `.environmentObject(model)`: a befecskendezett típus (a `@Environment(Store.self)`
    /// / `@EnvironmentObject` fogadó párja); kulcs-útvonalnál vagy ismeretlen típusnál `None`.
    pub(super) fn injected_type(
        &self,
        call: Node<'_>,
        name: &str,
        scope: &Scope,
    ) -> Option<String> {
        if !matches!(name, "environment" | "environmentObject") {
            return None;
        }
        let value = self.first_argument(call)?;
        match value.kind() {
            "simple_identifier" | "navigation_expression" => {
                self.receiver_type(&self.path_of(value)?, scope)
            }
            "call_expression" => scan::constructed_type(self.parsed, value),
            _ => None,
        }
    }

    /// Navigációs hívás-e (a zárójelén belül épített nézet / ViewModel navigációs él).
    pub(super) fn is_nav_call(&self, call: Node<'_>) -> bool {
        named_children(call)
            .into_iter()
            .next()
            .and_then(|expr| self.callee(expr))
            .is_some_and(|callee| NAV_CALLS.contains(&callee.name.as_str()))
    }

    /// A hívás neve, amelynek az argumentuma a `value_argument`.
    fn enclosing_callee(&self, argument: Node<'_>) -> Option<String> {
        let call = argument.parent()?.parent()?.parent()?;
        match call.kind() {
            "call_expression" => {
                let expr = named_children(call).into_iter().next()?;
                self.callee(expr).map(|callee| callee.name)
            }
            // `Box<Item>(onTap: vm.tap)`: a generikus konstruktor típusneve
            "constructor_expression" => self.constructed_name(call),
            _ => None,
        }
    }

    /// A `constructor_expression` típusneve (`Box<Item>(…)` → `Box`); tömb- / szótár-típusnál `None`.
    pub(super) fn constructed_name(&self, node: Node<'_>) -> Option<String> {
        let ty = node.child_by_field_name("constructed_type")?;
        if ty.kind() != "user_type" {
            return None;
        }
        named_children(ty)
            .into_iter()
            .rfind(|c| c.kind() == "type_identifier")
            .map(|n| self.text(n).to_string())
    }

    /// Argumentum-érték, amely metódus-hivatkozás lehet: `Composer(onSend: vm.send)`, `Button(action: save)`.
    /// A tulajdonság-olvasás (`Text(vm.title)`) nem hivatkozás: csak visszahívást jelző címke / hívás, vagy a
    /// fájlban definiált metódus neve alapján veszünk fel élt.
    pub(super) fn argument_ref(
        &mut self,
        argument: Node<'_>,
        scope: &Scope,
        b: &mut FileGraphBuilder,
    ) {
        let Some(value) = argument.child_by_field_name("value") else {
            return;
        };
        let label = argument
            .child_by_field_name("name")
            .map(|n| self.text(n).trim_end_matches(':').trim().to_string());
        let callee = self.enclosing_callee(argument).unwrap_or_default();
        let hint = match label.as_deref() {
            Some(label) if callback_like(label) => Hint::Label,
            Some(_) => Hint::None,
            None if callee_takes_callback(&callee) => Hint::Callee,
            None => Hint::None,
        };
        self.method_ref(value, hint, scope, b);
    }

    /// `#selector(handle)` / `#selector(vm.handle)` / `#selector(handle(_:))`: a megnevezett metódus
    /// hivatkozása (a belső hívás-szerű alakot nem járjuk be, az nem hívás).
    pub(super) fn selector_ref(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let Some(mut inner) = named_children(node).into_iter().next() else {
            return;
        };
        if inner.kind() == "call_expression" {
            match named_children(inner).into_iter().next() {
                Some(callee) => inner = callee,
                None => return,
            }
        }
        self.method_ref(inner, Hint::Label, scope, b);
    }

    /// Elfogadható-e `ty.name` metódus-hivatkozásnak. A fájlban deklarált típusnál a metódus-lista dönt: metódus
    /// mindig hivatkozás, tulajdonság (`vm.title`) csak címkével jelzett visszahívásnál (tárolt closure).
    /// Másik fájlban deklarált típusnál csak a visszahívást jelző szövegkörnyezet számít.
    fn accepts_ref(&self, ty: &str, name: &str, hint: Hint) -> bool {
        let key = scan::type_key(ty);
        if self
            .symbols
            .methods
            .get(key)
            .is_some_and(|methods| methods.contains(name))
        {
            return true;
        }
        let is_property = self
            .symbols
            .props
            .get(key)
            .is_some_and(|props| props.contains_key(name));
        match hint {
            Hint::Label => true,
            Hint::Callee => !is_property,
            Hint::None => false,
        }
    }

    /// Egy érték, amely metódusra / függvényre hivatkozik (nem hívja): hívás-él `handler: true` jelöléssel.
    /// `hint`: a szövegkörnyezet visszahívást vár; e nélkül csak a fájlban deklarált típus metódusa számít.
    fn method_ref(&mut self, value: Node<'_>, hint: Hint, scope: &Scope, b: &mut FileGraphBuilder) {
        if !matches!(value.kind(), "simple_identifier" | "navigation_expression") {
            return;
        }
        let Some(path) = self.path_of(value) else {
            return;
        };
        let (receiver, name) = match path.rsplit_once('.') {
            Some((receiver, name)) => (Some(receiver.to_string()), name.to_string()),
            None => (None, path.clone()),
        };
        let (resolved, root) = match &receiver {
            Some(receiver) => {
                let Some(ty) = self.receiver_type(receiver, scope) else {
                    return;
                };
                if !self.accepts_ref(&ty, &name, hint) {
                    return;
                }
                let root = receiver.split('.').next().map(str::to_string);
                (Some(format!("{ty}::{name}")), root)
            }
            None => {
                // puszta név: csak a fájlban definiált saját metódus (helyi változó nem)
                if !self.symbols.functions.contains(&name) || scope.locals.contains_key(&name) {
                    return;
                }
                (None, None)
            }
        };
        let on_self = receiver
            .as_deref()
            .is_some_and(|r| r == "self" || r == "Self");
        let member = receiver.is_some();
        let info = CallInfo {
            name: name.clone(),
            member,
            root: root.clone(),
            on_self,
            chained: false,
            in_view: scope.in_view,
            recv_type: None,
        };
        let category = self.categorizer.categorize(&info, &self.symbols);
        b.call(Call {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: line(value),
            end_line: None,
            kind: if member { "method_call" } else { "func_call" }.to_string(),
            target: path,
            resolved,
            on: root.filter(|_| member && !on_self),
            category: category.to_string(),
            args: None,
            handler: Some(true),
            from: None,
        });
    }

    /// `socket.onFrame = { … }` / `self.onTap = vm.save`: tárolt closure bekötése. Hívás-él `wire` fajtával:
    /// a célja a tulajdonság (`Típus::név`, ismeretlen fogadónál `?::név`), a sortartománya a closure — az
    /// index ebből köti a tulajdonságot meghívó metódust a closure hívásaihoz.
    pub(super) fn assignment_wire(
        &mut self,
        node: Node<'_>,
        scope: &Scope,
        b: &mut FileGraphBuilder,
    ) {
        let (Some(target), Some(result)) = (
            node.child_by_field_name("target"),
            node.child_by_field_name("result"),
        ) else {
            return;
        };
        let Some(lhs) = named_children(target).into_iter().next() else {
            return;
        };
        let Some(path) = self.path_of(lhs) else {
            return;
        };
        let (receiver, name) = match path.rsplit_once('.') {
            Some((receiver, name)) => (Some(receiver.to_string()), name.to_string()),
            None => (None, path.clone()),
        };
        let lambda = result.kind() == "lambda_literal";
        if !lambda && !callback_like(&name) {
            return;
        }
        let resolved = match &receiver {
            Some(receiver) => match self.receiver_type(receiver, scope) {
                Some(ty) => format!("{ty}::{name}"),
                None => format!("?::{name}"),
            },
            None => match (self.own_property(scope, &name), self.own_type(scope)) {
                (Some(_), Some(own)) => format!("{own}::{name}"),
                _ => return,
            },
        };
        let on_self = receiver.as_deref().is_none_or(|r| r == "self");
        let root = receiver
            .as_deref()
            .and_then(|r| r.split('.').next())
            .map(str::to_string);
        let info = CallInfo {
            name: name.clone(),
            member: true,
            root: root.clone(),
            on_self,
            chained: false,
            in_view: scope.in_view,
            recv_type: None,
        };
        let category = self.categorizer.categorize(&info, &self.symbols);
        let start = line(node);
        let end = end_line(node);
        b.call(Call {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: start,
            end_line: (end > start).then_some(end),
            kind: "wire".to_string(),
            target: path,
            resolved: Some(resolved),
            on: root.filter(|_| !on_self),
            category: category.to_string(),
            args: None,
            handler: None,
            from: None,
        });
        if !lambda {
            self.method_ref(result, Hint::Label, scope, b);
        }
    }
}
