//! Egyszerű fogadó-típus következtetés a metódushívásokhoz (`x.bar()` → `Típus::bar`).
//!
//! Csak olcsó, megbízható források: a paraméter típusa (`x: &Repo`, `State(s): State<App>`), a `let`
//! típusa (`let x: Repo = …`), a konstruktor-szerű hívás (`Repo::new(…)`, `Repo::with_x(…)?`), a struct-
//! literál (`Repo { … }`), a `self` (a container), és a saját struct mezőjének típusa (`self.repo.x()`).
//! Egy név csak akkor kap típust, ha a függvényben minden kötése ugyanazt a típust adja (árnyékolás,
//! closure-paraméter vagy minta-kötés azonos névvel → nincs típus).

use std::collections::{HashMap, HashSet};

use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{
    Block, Expr, ExprClosure, ExprForLoop, ExprLet, ExprMatch, ExprMethodCall, FnArg,
    GenericArgument, GenericParam, Local, Pat, PatIdent, Path, PathArguments, ReturnType,
    Signature, Token, Type, TypeParamBound, WherePredicate,
};

use super::flow::{Env, Fields, Item, Shape, closure_slot, item_of, shape_of_expr, shape_of_type};

/// A fájl függvényeinek visszatérési típusa: `(container, függvénynév)` → típus-út (`Result<T, _>` /
/// `Option<T>` → `T`, `Self` → `["Self"]`). A `let x = self.make()?` / `Svc::open()?` fogadóhoz.
pub(super) type Returns = HashMap<(String, String), Vec<String>>;

/// A visszatérési típus fogadóként használt útja (lásd `Returns`).
pub(super) fn return_path(sig: &Signature) -> Option<Vec<String>> {
    let ReturnType::Type(_, ty) = &sig.output else {
        return None;
    };
    let generics: HashSet<String> = sig
        .generics
        .type_params()
        .map(|p| p.ident.to_string())
        .collect();
    let inner = unwrap_wrapper(ty);
    type_path(inner, &generics).or_else(|| {
        matches!(inner, Type::Path(p) if p.path.is_ident("Self")).then(|| vec!["Self".to_owned()])
    })
}

/// `Result<T, E>` / `Option<T>` → `T` (más típus változatlan).
fn unwrap_wrapper(ty: &Type) -> &Type {
    let Type::Path(p) = ty else {
        return ty;
    };
    let Some(last) = p.path.segments.last() else {
        return ty;
    };
    if !matches!(last.ident.to_string().as_str(), "Result" | "Option") {
        return ty;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return ty;
    };
    match args.args.first() {
        Some(GenericArgument::Type(inner)) => inner,
        _ => ty,
    }
}

/// A következtetés környezete: a hívó container és a fájl függvényeinek visszatérési típusa.
pub(super) struct Infer<'g> {
    pub rets: &'g Returns,
    pub container: &'g str,
    /// A fájl struct-mezőinek alakja (`for b in self.bars.iter_mut()`).
    pub fields: &'g Fields,
}

/// Több saját trait-korlátú generikus (`T: A + B`) „típusa”: `["?", "A", "mod::B"]` — a jelölő után a
/// jelöltek `::`-szel összefűzve. A hívás helyén a metódust ténylegesen deklaráló trait dönt.
pub(super) const MULTI: &str = "?";

fn multi_path(candidates: &[Vec<String>]) -> Vec<String> {
    let mut out = vec![MULTI.to_owned()];
    out.extend(candidates.iter().map(|c| c.join("::")));
    out
}

/// A `T: A + B` jelölt-útjai; `None`, ha a típus nem ilyen.
pub(super) fn multi_candidates(ty: &[String]) -> Option<Vec<Vec<String>>> {
    let (first, rest) = ty.split_first()?;
    (first == MULTI).then(|| {
        rest.iter()
            .map(|c| c.split("::").map(str::to_owned).collect())
            .collect()
    })
}

/// Gyakori std-trait: generikus korlátként nem ad saját típust a fogadóhoz.
const STD_BOUNDS: [&str; 30] = [
    "Clone",
    "Copy",
    "Debug",
    "Display",
    "Default",
    "Send",
    "Sync",
    "Sized",
    "Eq",
    "PartialEq",
    "Ord",
    "PartialOrd",
    "Hash",
    "Into",
    "From",
    "TryFrom",
    "TryInto",
    "AsRef",
    "AsMut",
    "Iterator",
    "IntoIterator",
    "Fn",
    "FnMut",
    "FnOnce",
    "ToString",
    "Borrow",
    "Deref",
    "Future",
    "Serialize",
    "Deserialize",
];

/// A generikus paraméterek nem-std trait-korlátai (`T: Handler`, `where T: Handler + Clone`); két saját
/// trait esetén (`T: A + B`) mindkettő jelölt (lásd `MULTI`).
fn generic_bounds(sig: &Signature) -> HashMap<String, Vec<Vec<String>>> {
    let mut out: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    let mut add = |name: String, bounds: &Punctuated<TypeParamBound, Token![+]>| {
        let own: Vec<Vec<String>> = bounds
            .iter()
            .filter_map(|b| match b {
                TypeParamBound::Trait(t) => {
                    let segs = segments(&t.path);
                    let last = segs.last()?;
                    (!STD_BOUNDS.contains(&last.as_str())).then_some(segs)
                }
                _ => None,
            })
            .collect();
        if !own.is_empty() {
            out.entry(name).or_insert(own);
        }
    };
    for param in &sig.generics.params {
        if let GenericParam::Type(t) = param {
            add(t.ident.to_string(), &t.bounds);
        }
    }
    if let Some(clause) = &sig.generics.where_clause {
        for pred in &clause.predicates {
            if let WherePredicate::Type(p) = pred
                && let Type::Path(tp) = &p.bounded_ty
                && let Some(ident) = tp.path.get_ident()
            {
                add(ident.to_string(), &p.bounds);
            }
        }
    }
    out
}

/// A típus egy generikus paraméter (`&T`, `Box<T>`) — a korlátja szerinti trait-út.
fn bound_of(ty: &Type, bounds: &HashMap<String, Vec<Vec<String>>>) -> Option<Vec<String>> {
    match ty {
        Type::Reference(r) => bound_of(&r.elem, bounds),
        Type::Paren(p) => bound_of(&p.elem, bounds),
        Type::Path(p) if p.qself.is_none() => {
            let last = p.path.segments.last()?;
            if matches!(last.ident.to_string().as_str(), "Box" | "Rc" | "Arc") {
                let PathArguments::AngleBracketed(args) = &last.arguments else {
                    return None;
                };
                return match args.args.first() {
                    Some(GenericArgument::Type(inner)) => bound_of(inner, bounds),
                    _ => None,
                };
            }
            let found = bounds.get(&p.path.get_ident()?.to_string())?;
            match found.as_slice() {
                [only] => Some(only.clone()),
                many => Some(multi_path(many)),
            }
        }
        _ => None,
    }
}

/// A függvény helyi neveinek típus-útja (a leírt alak szegmensei, generikus argumentumok nélkül).
#[derive(Debug, Default, Clone)]
pub(super) struct TypeEnv(HashMap<String, Vec<String>>);

impl TypeEnv {
    /// A függvény paramétereiből és `let`-jeiből.
    pub(super) fn of_fn(sig: &Signature, block: &Block, cx: &Infer<'_>) -> Self {
        let generics: HashSet<String> = sig
            .generics
            .type_params()
            .map(|p| p.ident.to_string())
            .collect();
        let bounds = generic_bounds(sig);
        let mut scan = Scan {
            generics: &generics,
            bounds: &bounds,
            cx,
            typed: HashMap::new(),
            shapes: HashMap::new(),
            counts: HashMap::new(),
        };
        for input in &sig.inputs {
            match input {
                FnArg::Receiver(_) => {
                    scan.typed
                        .insert("self".to_owned(), vec![Some(vec!["Self".to_owned()])]);
                    scan.counts.insert("self".to_owned(), 1);
                }
                FnArg::Typed(t) => {
                    scan.param(&t.pat, &t.ty);
                    scan.visit_pat(&t.pat);
                }
            }
        }
        scan.visit_block(block);
        let Scan { typed, counts, .. } = scan;
        let env = typed
            .into_iter()
            .filter_map(|(name, types)| {
                let first = types.first()?.clone()?;
                let agree = types.iter().all(|t| t.as_ref() == Some(&first));
                let all_typed = counts.get(&name).copied().unwrap_or(0) == types.len();
                (agree && all_typed).then_some((name, first))
            })
            .collect();
        Self(env)
    }

    pub(super) fn get(&self, name: &str) -> Option<&Vec<String>> {
        self.0.get(name)
    }
}

/// A kötések gyűjtése: `typed` = a név egyszerű kötéseinek (következtetett) típusai, `counts` = a név
/// összes kötése (minden minta-azonosító, a closure-paraméterek és a beágyazott minták is).
struct Scan<'g> {
    generics: &'g HashSet<String>,
    /// generikus paraméter → a trait-korlátja (`T: Handler` → `Handler`)
    bounds: &'g HashMap<String, Vec<Vec<String>>>,
    cx: &'g Infer<'g>,
    typed: HashMap<String, Vec<Option<Vec<String>>>>,
    /// a név egyszerű kötéseinek gyűjtemény- / iterátor- / `Option`-alakja
    shapes: HashMap<String, Vec<Option<Shape>>>,
    counts: HashMap<String, usize>,
}

impl Scan<'_> {
    /// A típus fogadó-útja: saját típus, vagy a generikus paraméter trait-korlátja.
    fn ty_path(&self, ty: &Type) -> Option<Vec<String>> {
        type_path(ty, self.generics).or_else(|| bound_of(ty, self.bounds))
    }

    fn bind(&mut self, name: String, ty: Option<Vec<String>>) {
        self.typed.entry(name).or_default().push(ty);
    }

    fn bind_shape(&mut self, name: &str, shape: Option<Shape>) {
        self.shapes.entry(name.to_owned()).or_default().push(shape);
    }

    /// A típus gyűjtemény- / `Option`-alakja (az elem-típusok a `ty_path` szerint).
    fn shape_of_ty(&self, ty: &Type) -> Option<Shape> {
        shape_of_type(ty, &|t| self.ty_path(t))
    }

    /// Egy minta kötéseinek típusa az elem alakjából: `x`, `&x`, `(k, v)`, `(i, x)`.
    fn bind_pat(&mut self, pat: &Pat, item: &Item) {
        match pat {
            Pat::Ident(p) if p.subpat.is_none() => {
                let ty = match item {
                    Item::One(ty) => ty.clone(),
                    Item::Tuple(_) => None,
                };
                self.bind(p.ident.to_string(), ty);
            }
            Pat::Reference(r) => self.bind_pat(&r.pat, item),
            Pat::Paren(p) => self.bind_pat(&p.pat, item),
            Pat::Tuple(t) => {
                if let Item::Tuple(items) = item
                    && items.len() == t.elems.len()
                {
                    for (pat, item) in t.elems.iter().zip(items) {
                        self.bind_pat(pat, item);
                    }
                }
            }
            _ => {}
        }
    }

    /// `Some(x)` / `Ok(x)` minta egy ismert `Option` / `Result` / `.next()` / `.get(k)` kifejezésen.
    fn bind_option_pat(&mut self, pat: &Pat, scrutinee: &Expr) {
        let Pat::TupleStruct(ts) = pat else {
            return;
        };
        let wrapper = ts.path.segments.last().map(|s| s.ident.to_string());
        if !matches!(wrapper.as_deref(), Some("Some" | "Ok")) || ts.elems.len() != 1 {
            return;
        }
        let Some(inner) = ts.elems.first() else {
            return;
        };
        if let Some(Shape::Opt(item)) = shape_of_expr(scrutinee, &*self) {
            self.bind_pat(inner, &item);
        }
    }

    /// Az iterátor-adapter closure-paramétere (`items.iter().map(|x| …)`, `fold(0, |acc, x| …)`) az elemet kapja.
    fn closure_params(&mut self, node: &ExprMethodCall) {
        let Some(recv) = shape_of_expr(&node.receiver, &*self) else {
            return;
        };
        let name = node.method.to_string();
        let Some((index, all)) = closure_slot(&name, matches!(recv, Shape::Coll { .. })) else {
            return;
        };
        let Some(Expr::Closure(closure)) = node
            .args
            .iter()
            .rev()
            .find(|a| matches!(a, Expr::Closure(_)))
        else {
            return;
        };
        let item = item_of(&recv).clone();
        let skip = if all { 0 } else { index };
        let take = if all { usize::MAX } else { 1 };
        for pat in closure.inputs.iter().skip(skip).take(take) {
            if !matches!(pat, Pat::Type(_)) {
                self.bind_pat(pat, &item);
            }
        }
    }

    /// Paraméter: `x: T`, vagy az extractor-szerű újracsomagolás `State(x): State<T>` → `x: T`.
    fn param(&mut self, pat: &Pat, ty: &Type) {
        match pat {
            Pat::Ident(p) if p.subpat.is_none() => {
                let path = self.ty_path(ty);
                let shape = self.shape_of_ty(ty);
                self.bind(p.ident.to_string(), path);
                self.bind_shape(&p.ident.to_string(), shape);
            }
            Pat::TupleStruct(ts) if ts.elems.len() == 1 => {
                let Some(Pat::Ident(inner)) = ts.elems.first() else {
                    return;
                };
                let wrapper = ts.path.segments.last().map(|s| s.ident.to_string());
                let ty = newtype_inner(ty, wrapper.as_deref())
                    .and_then(|inner_ty| self.ty_path(inner_ty));
                self.bind(inner.ident.to_string(), ty);
            }
            _ => {}
        }
    }
}

impl Env for Scan<'_> {
    /// A név alakja, ha eddigi minden kötése ugyanazt az alakot adja.
    fn var(&self, name: &str) -> Option<Shape> {
        let entries = self.shapes.get(name)?;
        let first = entries.first()?.clone()?;
        let bound = self.counts.get(name).copied().unwrap_or(0);
        (entries.len() == bound && entries.iter().all(|s| s.as_ref() == Some(&first)))
            .then_some(first)
    }

    fn field(&self, name: &str) -> Option<Shape> {
        self.cx
            .fields
            .get(&(self.cx.container.to_owned(), name.to_owned()))
            .cloned()
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_pat_ident(&mut self, node: &'ast PatIdent) {
        *self.counts.entry(node.ident.to_string()).or_default() += 1;
        visit::visit_pat_ident(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast ExprForLoop) {
        if let Some(shape) = shape_of_expr(&node.expr, &*self) {
            let item = item_of(&shape).clone();
            self.bind_pat(&node.pat, &item);
        }
        visit::visit_expr_for_loop(self, node);
    }

    fn visit_expr_let(&mut self, node: &'ast ExprLet) {
        self.bind_option_pat(&node.pat, &node.expr);
        visit::visit_expr_let(self, node);
    }

    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        for arm in &node.arms {
            self.bind_option_pat(&arm.pat, &node.expr);
        }
        visit::visit_expr_match(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        self.closure_params(node);
        visit::visit_expr_method_call(self, node);
    }

    /// Típusos closure-paraméter (`|f: Foo| f.run()`).
    fn visit_expr_closure(&mut self, node: &'ast ExprClosure) {
        for input in &node.inputs {
            if let Pat::Type(pt) = input {
                let item = Item::One(self.ty_path(&pt.ty));
                self.bind_pat(&pt.pat, &item);
            }
        }
        visit::visit_expr_closure(self, node);
    }

    fn visit_local(&mut self, node: &'ast Local) {
        let (ident, ty, shape) = match &node.pat {
            Pat::Ident(p) if p.subpat.is_none() => {
                let init = node.init.as_ref().map(|init| &*init.expr);
                let ty = init.and_then(|expr| infer_expr(expr, self.cx));
                let shape = init.and_then(|expr| shape_of_expr(expr, &*self));
                (Some(&p.ident), ty, shape)
            }
            Pat::Type(pt) => match &*pt.pat {
                Pat::Ident(p) if p.subpat.is_none() => (
                    Some(&p.ident),
                    self.ty_path(&pt.ty),
                    self.shape_of_ty(&pt.ty),
                ),
                _ => (None, None, None),
            },
            Pat::TupleStruct(_) => {
                // `let Some(x) = opt else { return };`
                if let Some(init) = &node.init {
                    self.bind_option_pat(&node.pat, &init.expr);
                }
                (None, None, None)
            }
            _ => (None, None, None),
        };
        if let Some(ident) = ident {
            self.bind(ident.to_string(), ty);
            self.bind_shape(&ident.to_string(), shape);
        }
        visit::visit_local(self, node);
    }
}

/// `State<T>` a `State(x)` mintához: a csomagoló neve egyezik, és egyetlen típus-argumentuma van.
fn newtype_inner<'t>(ty: &'t Type, wrapper: Option<&str>) -> Option<&'t Type> {
    let Type::Path(p) = ty else {
        return None;
    };
    let last = p.path.segments.last()?;
    if Some(last.ident.to_string().as_str()) != wrapper {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let mut types = args.args.iter().filter_map(|a| match a {
        GenericArgument::Type(t) => Some(t),
        _ => None,
    });
    let first = types.next()?;
    types.next().is_none().then_some(first)
}

/// Egy út szegmensei (generikus argumentumok nélkül).
pub(super) fn segments(path: &Path) -> Vec<String> {
    path.segments.iter().map(|s| s.ident.to_string()).collect()
}

fn starts_upper(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// Egy típus fogadóként használt útja: `&T`, `Box<T>` / `Rc<T>` / `Arc<T>` → `T`; `dyn Trait` /
/// `impl Trait` → a trait. Generikus paraméter (és egybetűs név) nem ad típust.
pub(super) fn type_path(ty: &Type, generics: &HashSet<String>) -> Option<Vec<String>> {
    match ty {
        Type::Reference(r) => type_path(&r.elem, generics),
        Type::Paren(p) => type_path(&p.elem, generics),
        Type::Group(g) => type_path(&g.elem, generics),
        Type::Path(p) if p.qself.is_none() => {
            let last = p.path.segments.last()?;
            if matches!(last.ident.to_string().as_str(), "Box" | "Rc" | "Arc") {
                let PathArguments::AngleBracketed(args) = &last.arguments else {
                    return None;
                };
                return match args.args.first() {
                    Some(GenericArgument::Type(inner)) => type_path(inner, generics),
                    _ => None,
                };
            }
            let segs = segments(&p.path);
            let generic =
                segs.len() == 1 && (generics.contains(&segs[0]) || segs[0].chars().count() == 1);
            (!generic).then_some(segs)
        }
        Type::ImplTrait(t) => first_trait(&t.bounds),
        Type::TraitObject(t) => first_trait(&t.bounds),
        _ => None,
    }
}

fn first_trait(bounds: &Punctuated<TypeParamBound, Token![+]>) -> Option<Vec<String>> {
    bounds.iter().find_map(|b| match b {
        TypeParamBound::Trait(t) => Some(segments(&t.path)),
        _ => None,
    })
}

/// Konstruktor-szerű társított függvény: `new`, `default`, `from`, `new_*`, `with_*`, `from_*`.
fn is_constructor(name: &str) -> bool {
    matches!(name, "new" | "default" | "from")
        || name.starts_with("new_")
        || name.starts_with("with_")
        || name.starts_with("from_")
}

/// A `let` kezdőértékének típusa: struct-literál, konstruktor-hívás, a fájl függvényének ismert
/// visszatérési típusa (`self.make()?`, `Svc::open()?`); `?`, `.unwrap()`, `.expect()`, `.await`,
/// zárójel / referencia átlátszó.
pub(super) fn infer_expr(expr: &Expr, cx: &Infer<'_>) -> Option<Vec<String>> {
    match expr {
        Expr::Struct(s) if s.qself.is_none() => {
            let segs = segments(&s.path);
            // `Enum::Variant { … }` → az enum
            match segs.as_slice() {
                [.., owner, variant] if starts_upper(owner) && starts_upper(variant) => {
                    Some(segs[..segs.len() - 1].to_vec())
                }
                _ => Some(segs),
            }
        }
        Expr::Call(call) => {
            let Expr::Path(p) = &*call.func else {
                return None;
            };
            if p.qself.is_some() {
                return None;
            }
            let segs = segments(&p.path);
            let [.., owner, func] = segs.as_slice() else {
                return None;
            };
            if owner != "Self" && !starts_upper(owner) {
                return None;
            }
            let owner_path = segs[..segs.len() - 1].to_vec();
            if is_constructor(func) {
                return Some(owner_path);
            }
            let container = if owner == "Self" { cx.container } else { owner };
            let ret = cx.rets.get(&(container.to_owned(), func.clone()))?;
            Some(if ret.len() == 1 && ret[0] == "Self" {
                owner_path
            } else {
                ret.clone()
            })
        }
        Expr::Try(t) => infer_expr(&t.expr, cx),
        Expr::Await(a) => infer_expr(&a.base, cx),
        Expr::Paren(p) => infer_expr(&p.expr, cx),
        Expr::Group(g) => infer_expr(&g.expr, cx),
        Expr::Reference(r) => infer_expr(&r.expr, cx),
        Expr::MethodCall(m) if m.method == "unwrap" || m.method == "expect" => {
            infer_expr(&m.receiver, cx)
        }
        Expr::MethodCall(m) if matches!(&*m.receiver, Expr::Path(p) if p.path.is_ident("self")) => {
            cx.rets
                .get(&(cx.container.to_owned(), m.method.to_string()))
                .cloned()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(src: &str) -> TypeEnv {
        let f: syn::ItemFn = syn::parse_str(src).unwrap();
        let rets = Returns::new();
        TypeEnv::of_fn(
            &f.sig,
            &f.block,
            &Infer {
                rets: &rets,
                container: "Own",
                fields: &Fields::new(),
            },
        )
    }

    fn ty(env: &TypeEnv, name: &str) -> Option<String> {
        env.get(name).map(|segs| segs.join("::"))
    }

    #[test]
    fn params_lets_and_constructors() {
        let e = env(
            "fn f<T: Store>(a: &Repo, b: Box<dyn Shape>, c: T, State(s): State<App>, g: Arc<db::Pool>) {
                let x = Repo::new(1);
                let y: Cache = make();
                let z = Config { a: 1 };
                let w = Client::with_timeout(5)?;
                let v = Msg::Quit { code: 1 };
                let u = helper();
            }",
        );
        assert_eq!(ty(&e, "a").as_deref(), Some("Repo"));
        assert_eq!(ty(&e, "b").as_deref(), Some("Shape"));
        // generikus paraméter: a saját trait-korlát a típus (`T: Store` → Store)
        assert_eq!(ty(&e, "c").as_deref(), Some("Store"));
        assert_eq!(ty(&e, "s").as_deref(), Some("App"));
        assert_eq!(ty(&e, "g").as_deref(), Some("db::Pool"));
        assert_eq!(ty(&e, "x").as_deref(), Some("Repo"));
        assert_eq!(ty(&e, "y").as_deref(), Some("Cache"));
        assert_eq!(ty(&e, "z").as_deref(), Some("Config"));
        assert_eq!(ty(&e, "w").as_deref(), Some("Client"));
        assert_eq!(ty(&e, "v").as_deref(), Some("Msg"));
        assert_eq!(ty(&e, "u"), None);
    }

    #[test]
    fn shadowing_and_other_bindings_drop_the_type() {
        let e = env("fn f(r: Repo, items: Vec<u8>) {
                let x = Repo::new();
                for x in items {}
                let y = Repo::new();
                let y = Cache::new();
                let z = Repo::new();
                let z = Repo::default();
                let c = |r| r;
            }");
        assert_eq!(ty(&e, "x"), None);
        assert_eq!(ty(&e, "y"), None);
        assert_eq!(ty(&e, "z").as_deref(), Some("Repo"));
        assert_eq!(ty(&e, "r"), None);
    }

    #[test]
    fn self_receiver_is_self() {
        let item: syn::ImplItemFn = syn::parse_str("fn f(&self) { self.go(); }").unwrap();
        let rets = Returns::new();
        let fields = Fields::new();
        let cx = Infer {
            rets: &rets,
            container: "Own",
            fields: &fields,
        };
        let e = TypeEnv::of_fn(&item.sig, &item.block, &cx);
        assert_eq!(ty(&e, "self").as_deref(), Some("Self"));
    }

    #[test]
    fn generic_trait_bounds_give_the_trait() {
        let e = env(
            "fn f<T: Handler, U: Clone, V>(a: &T, b: U, c: Box<V>, d: &W) where V: Store + Send { }",
        );
        assert_eq!(ty(&e, "a").as_deref(), Some("Handler"));
        assert_eq!(ty(&e, "b"), None);
        assert_eq!(ty(&e, "c").as_deref(), Some("Store"));
        assert_eq!(ty(&e, "d"), None);
    }

    #[test]
    fn known_return_types_type_the_binding() {
        let mut rets = Returns::new();
        rets.insert(("Svc".into(), "make".into()), vec!["Bar".into()]);
        rets.insert(("Svc".into(), "open".into()), vec!["Self".into()]);
        let fields = Fields::new();
        let cx = Infer {
            rets: &rets,
            container: "Svc",
            fields: &fields,
        };
        let f: syn::ImplItemFn = syn::parse_str(
            "fn go(&self) { let a = self.make().unwrap(); let b = Svc::open()?; let c = Self::open(); }",
        )
        .unwrap();
        let e = TypeEnv::of_fn(&f.sig, &f.block, &cx);
        assert_eq!(ty(&e, "a").as_deref(), Some("Bar"));
        assert_eq!(ty(&e, "b").as_deref(), Some("Svc"));
        assert_eq!(ty(&e, "c").as_deref(), Some("Self"));
    }

    #[test]
    fn return_paths_unwrap_result_and_option() {
        let path = |src: &str| {
            let f: syn::ItemFn = syn::parse_str(src).unwrap();
            return_path(&f.sig).map(|p| p.join("::"))
        };
        assert_eq!(path("fn a() -> Result<Bar, E> {}").as_deref(), Some("Bar"));
        assert_eq!(path("fn a() -> Option<Self> {}").as_deref(), Some("Self"));
        assert_eq!(path("fn a() -> io::Result<Bar> {}").as_deref(), Some("Bar"));
        assert_eq!(path("fn a() -> i32 {}").as_deref(), Some("i32"));
        assert_eq!(path("fn a<T>() -> T {}"), None);
        assert_eq!(path("fn a() {}"), None);
    }
}
