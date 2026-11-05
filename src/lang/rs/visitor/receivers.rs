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
    Block, Expr, FnArg, GenericArgument, Local, Pat, PatIdent, Path, PathArguments, Signature,
    Token, Type, TypeParamBound,
};

/// A függvény helyi neveinek típus-útja (a leírt alak szegmensei, generikus argumentumok nélkül).
#[derive(Debug, Default, Clone)]
pub(super) struct TypeEnv(HashMap<String, Vec<String>>);

impl TypeEnv {
    /// A függvény paramétereiből és `let`-jeiből.
    pub(super) fn of_fn(sig: &Signature, block: &Block) -> Self {
        let generics: HashSet<String> = sig
            .generics
            .type_params()
            .map(|p| p.ident.to_string())
            .collect();
        let mut scan = Scan {
            generics: &generics,
            typed: HashMap::new(),
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
    typed: HashMap<String, Vec<Option<Vec<String>>>>,
    counts: HashMap<String, usize>,
}

impl Scan<'_> {
    fn bind(&mut self, name: String, ty: Option<Vec<String>>) {
        self.typed.entry(name).or_default().push(ty);
    }

    /// Paraméter: `x: T`, vagy az extractor-szerű újracsomagolás `State(x): State<T>` → `x: T`.
    fn param(&mut self, pat: &Pat, ty: &Type) {
        match pat {
            Pat::Ident(p) if p.subpat.is_none() => {
                let ty = type_path(ty, self.generics);
                self.bind(p.ident.to_string(), ty);
            }
            Pat::TupleStruct(ts) if ts.elems.len() == 1 => {
                let Some(Pat::Ident(inner)) = ts.elems.first() else {
                    return;
                };
                let wrapper = ts.path.segments.last().map(|s| s.ident.to_string());
                let ty = newtype_inner(ty, wrapper.as_deref())
                    .and_then(|inner_ty| type_path(inner_ty, self.generics));
                self.bind(inner.ident.to_string(), ty);
            }
            _ => {}
        }
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_pat_ident(&mut self, node: &'ast PatIdent) {
        *self.counts.entry(node.ident.to_string()).or_default() += 1;
        visit::visit_pat_ident(self, node);
    }

    fn visit_local(&mut self, node: &'ast Local) {
        let (ident, ty) = match &node.pat {
            Pat::Ident(p) if p.subpat.is_none() => {
                let ty = node.init.as_ref().and_then(|init| infer_expr(&init.expr));
                (Some(&p.ident), ty)
            }
            Pat::Type(pt) => match &*pt.pat {
                Pat::Ident(p) if p.subpat.is_none() => {
                    (Some(&p.ident), type_path(&pt.ty, self.generics))
                }
                _ => (None, None),
            },
            _ => (None, None),
        };
        if let Some(ident) = ident {
            self.bind(ident.to_string(), ty);
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

/// A `let` kezdőértékének típusa: struct-literál, konstruktor-hívás (`?`, `.unwrap()`, `.expect()`,
/// `.await` és zárójel / referencia átlátszó).
pub(super) fn infer_expr(expr: &Expr) -> Option<Vec<String>> {
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
            let typeish = owner == "Self" || starts_upper(owner);
            (typeish && is_constructor(func)).then(|| segs[..segs.len() - 1].to_vec())
        }
        Expr::Try(t) => infer_expr(&t.expr),
        Expr::Await(a) => infer_expr(&a.base),
        Expr::Paren(p) => infer_expr(&p.expr),
        Expr::Group(g) => infer_expr(&g.expr),
        Expr::Reference(r) => infer_expr(&r.expr),
        Expr::MethodCall(m) if m.method == "unwrap" || m.method == "expect" => {
            infer_expr(&m.receiver)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(src: &str) -> TypeEnv {
        let f: syn::ItemFn = syn::parse_str(src).unwrap();
        TypeEnv::of_fn(&f.sig, &f.block)
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
        assert_eq!(ty(&e, "c"), None);
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
        let e = TypeEnv::of_fn(&item.sig, &item.block);
        assert_eq!(ty(&e, "self").as_deref(), Some("Self"));
    }
}
