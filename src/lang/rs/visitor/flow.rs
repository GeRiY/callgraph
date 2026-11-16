//! Elem-típus következtetés a gyűjteményekből és iterátorokból: ciklusváltozó (`for b in self.bars.iter_mut()`),
//! `if let Some(x) = self.opt`, closure-paraméter az iterátor-adapterekben (`items.iter().map(|x| x.name())`).
//!
//! A kifejezés alakja (`Shape`) csak annyit tud, amennyi az elem-típus levezetéséhez kell: gyűjtemény
//! (iterált elem és a `get` / `first` értéke), iterátor (elem) és `Option` / `Result` (a tartalma).

use std::collections::HashMap;

use syn::{Expr, GenericArgument, Member, PathArguments, PathSegment, Type};

/// Egy típus fogadóként használt útja (a `receivers` szegmens-alakja).
pub(super) type TypePath = Vec<String>;

/// Egy elem alakja: egyetlen (talán ismeretlen) típus, vagy tuple (`(k, v)`, `enumerate`).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Item {
    One(Option<TypePath>),
    Tuple(Vec<Item>),
}

/// Egy kifejezés iterálható / kicsomagolható alakja.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Shape {
    /// `Vec<T>`, `&[T]`, `HashMap<K, V>`…: az iterált elem és a `get` / `first` / `pop` értéke.
    Coll { item: Item, get: Item },
    /// Iterátor: az elem.
    Iter(Item),
    /// `Option<T>` / `Result<T, _>`: a tartalom.
    Opt(Item),
}

/// A struct-mezők alakja: `(container, mező)` → alak (`self.bars.iter_mut()`-hez).
pub(super) type Fields = HashMap<(String, String), Shape>;

/// A névfeloldás, amit a kifejezés-következtetés kérdez: helyi változó és a `self` mezője.
pub(super) trait Env {
    fn var(&self, name: &str) -> Option<Shape>;
    fn field(&self, name: &str) -> Option<Shape>;
}

fn seq(elem: Option<TypePath>) -> Shape {
    Shape::Coll {
        item: Item::One(elem.clone()),
        get: Item::One(elem),
    }
}

/// A szegmens típus-argumentumai (`Vec<T>` → `[T]`).
fn type_args(seg: &PathSegment) -> Vec<&Type> {
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return Vec::new();
    };
    args.args
        .iter()
        .filter_map(|a| match a {
            GenericArgument::Type(t) => Some(t),
            _ => None,
        })
        .collect()
}

/// Egy típus alakja: `Vec<T>` / `[T; N]` / `&[T]` / `HashSet<T>` → gyűjtemény, `HashMap<K, V>` → pár,
/// `Option<T>` / `Result<T, _>` → tartalom, `Box` / `Rc` / `Arc` átlátszó. `elem` az elem-típus útja.
pub(super) fn shape_of_type(ty: &Type, elem: &dyn Fn(&Type) -> Option<TypePath>) -> Option<Shape> {
    match ty {
        Type::Reference(r) => shape_of_type(&r.elem, elem),
        Type::Paren(p) => shape_of_type(&p.elem, elem),
        Type::Group(g) => shape_of_type(&g.elem, elem),
        Type::Array(a) => Some(seq(elem(a.elem.as_ref()))),
        Type::Slice(s) => Some(seq(elem(s.elem.as_ref()))),
        Type::Path(p) if p.qself.is_none() => {
            let last = p.path.segments.last()?;
            let args = type_args(last);
            match last.ident.to_string().as_str() {
                "Vec" | "VecDeque" | "HashSet" | "BTreeSet" | "BinaryHeap" | "LinkedList"
                | "IndexSet" => Some(seq(elem(*args.first()?))),
                "HashMap" | "BTreeMap" | "IndexMap" => {
                    let key = elem(*args.first()?);
                    let val = elem(*args.get(1)?);
                    Some(Shape::Coll {
                        item: Item::Tuple(vec![Item::One(key), Item::One(val.clone())]),
                        get: Item::One(val),
                    })
                }
                "Option" | "Result" => Some(Shape::Opt(Item::One(elem(*args.first()?)))),
                "Box" | "Rc" | "Arc" => shape_of_type(args.first()?, elem),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Az iterálás eleme (`for x in …`): a gyűjtemény eleme, az iterátor eleme, az `Option` tartalma.
pub(super) fn item_of(shape: &Shape) -> &Item {
    match shape {
        Shape::Coll { item, .. } | Shape::Iter(item) | Shape::Opt(item) => item,
    }
}

/// Egy kifejezés alakja: helyi név, `self.mező`, `&` / zárójel, és a metódus-láncok (`.iter_mut()`, `.values()`,
/// `.enumerate()`, `.next()`, `.get(k)`…).
pub(super) fn shape_of_expr(expr: &Expr, env: &dyn Env) -> Option<Shape> {
    match expr {
        Expr::Path(p) if p.qself.is_none() => env.var(&p.path.get_ident()?.to_string()),
        Expr::Paren(p) => shape_of_expr(&p.expr, env),
        Expr::Group(g) => shape_of_expr(&g.expr, env),
        Expr::Reference(r) => shape_of_expr(&r.expr, env),
        Expr::Field(f) => {
            let Member::Named(name) = &f.member else {
                return None;
            };
            let Expr::Path(base) = &*f.base else {
                return None;
            };
            base.path
                .is_ident("self")
                .then(|| env.field(&name.to_string()))?
        }
        Expr::MethodCall(m) => {
            let recv = shape_of_expr(&m.receiver, env)?;
            method_shape(&m.method.to_string(), recv)
        }
        _ => None,
    }
}

/// A metódus eredményének alakja a fogadó alakjából.
fn method_shape(name: &str, recv: Shape) -> Option<Shape> {
    match (&recv, name) {
        (Shape::Coll { item, .. }, "iter" | "iter_mut" | "into_iter" | "drain") => {
            Some(Shape::Iter(item.clone()))
        }
        (Shape::Coll { item, .. }, "values" | "values_mut" | "into_values") => {
            let Item::Tuple(kv) = item else {
                return None;
            };
            Some(Shape::Iter(kv.get(1)?.clone()))
        }
        (Shape::Coll { item, .. }, "keys" | "into_keys") => {
            let Item::Tuple(kv) = item else {
                return None;
            };
            Some(Shape::Iter(kv.first()?.clone()))
        }
        (
            Shape::Coll { get, .. },
            "get" | "get_mut" | "first" | "last" | "first_mut" | "last_mut" | "front" | "back"
            | "front_mut" | "back_mut" | "pop" | "pop_front" | "pop_back" | "peek",
        ) => Some(Shape::Opt(get.clone())),
        (Shape::Iter(item), "iter" | "into_iter" | "by_ref") => Some(Shape::Iter(item.clone())),
        (
            Shape::Iter(item),
            "next" | "next_back" | "peek" | "peek_mut" | "nth" | "last" | "min" | "max" | "find"
            | "min_by_key" | "max_by_key" | "min_by" | "max_by",
        ) => Some(Shape::Opt(item.clone())),
        (
            Shape::Iter(item),
            "rev" | "skip" | "take" | "filter" | "cloned" | "copied" | "skip_while" | "take_while"
            | "peekable" | "step_by" | "inspect" | "fuse" | "chain",
        ) => Some(Shape::Iter(item.clone())),
        (Shape::Iter(item), "enumerate") => Some(Shape::Iter(Item::Tuple(vec![
            Item::One(None),
            item.clone(),
        ]))),
        (Shape::Opt(item), "iter" | "iter_mut" | "into_iter") => Some(Shape::Iter(item.clone())),
        (
            Shape::Opt(_),
            "cloned" | "copied" | "as_ref" | "as_mut" | "as_deref" | "as_deref_mut" | "clone"
            | "take" | "filter",
        ) => Some(recv.clone()),
        _ => None,
    }
}

/// A closure-paraméterek elem-kötése egy adapter-hívásnál: `(a paraméter indexe, az összes paraméter
/// ugyanaz-e)`; ismeretlen metódusnál `None`. `coll` = a fogadó közvetlenül gyűjtemény.
pub(super) fn closure_slot(name: &str, coll: bool) -> Option<(usize, bool)> {
    if coll {
        return match name {
            "retain" | "retain_mut" | "sort_by_key" | "sort_unstable_by_key" | "dedup_by_key" => {
                Some((0, false))
            }
            "sort_by" | "sort_unstable_by" | "dedup_by" => Some((0, true)),
            _ => None,
        };
    }
    match name {
        "fold" | "try_fold" => Some((1, false)),
        "max_by" | "min_by" => Some((0, true)),
        "map" | "filter" | "filter_map" | "flat_map" | "for_each" | "any" | "all" | "find"
        | "find_map" | "position" | "rposition" | "take_while" | "skip_while" | "map_while"
        | "inspect" | "max_by_key" | "min_by_key" | "partition" | "is_some_and" | "and_then"
        | "map_or" | "map_or_else" | "is_ok_and" => Some((0, false)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elem(ty: &Type) -> Option<TypePath> {
        match ty {
            Type::Path(p) => Some(super::super::receivers::segments(&p.path)),
            Type::Reference(r) => elem(&r.elem),
            _ => None,
        }
    }

    fn shape(src: &str) -> Option<Shape> {
        let ty: Type = syn::parse_str(src).unwrap();
        shape_of_type(&ty, &elem)
    }

    fn one(name: &str) -> Item {
        Item::One(Some(vec![name.to_owned()]))
    }

    #[test]
    fn container_types_have_shapes() {
        assert_eq!(item_of(&shape("Vec<Bar>").unwrap()), &one("Bar"));
        assert_eq!(item_of(&shape("&[Bar]").unwrap()), &one("Bar"));
        assert_eq!(item_of(&shape("[Bar; 3]").unwrap()), &one("Bar"));
        assert_eq!(item_of(&shape("Option<Bar>").unwrap()), &one("Bar"));
        assert_eq!(
            item_of(&shape("HashMap<String, Bar>").unwrap()),
            &Item::Tuple(vec![one("String"), one("Bar")])
        );
        assert_eq!(shape("Foo"), None);
    }
}
