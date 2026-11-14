//! Típus-leképezés: a nyelvfüggetlen `TypeRef` TypeScript-típussá, Vue futásidejű prop-típussá (`String`, `Array` …)
//! és értelmes alapértékké (`ref<number>(0)`).

use crate::generate::ast::TypeRef;

/// A TypeScript-típus: `int`/`float` → `number`, `list<T>` → `T[]`, `map<K,V>` → `Record<K, V>`, `opt<T>` → `T | null`.
pub fn ts_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Int | TypeRef::Float => "number".to_owned(),
        TypeRef::Bool => "boolean".to_owned(),
        TypeRef::Str | TypeRef::Bytes => "string".to_owned(),
        TypeRef::Void => "void".to_owned(),
        TypeRef::Mixed => "unknown".to_owned(),
        TypeRef::List(inner) => {
            let item = ts_type(inner);
            if item.contains(" | ") || item.contains("=>") {
                format!("({item})[]")
            } else {
                format!("{item}[]")
            }
        }
        TypeRef::Map(k, v) => format!("Record<{}, {}>", ts_type(k), ts_type(v)),
        TypeRef::Iter(inner) => format!("Iterable<{}>", ts_type(inner)),
        TypeRef::Opt(inner) => {
            let item = ts_type(inner);
            if item.ends_with(" | null") {
                item
            } else {
                format!("{item} | null")
            }
        }
        TypeRef::Union(items) => items.iter().map(ts_type).collect::<Vec<_>>().join(" | "),
        TypeRef::Named(name) => name.clone(),
    }
}

/// A Vue futásidejű prop-típus (`type: String`); `None` = nincs megkötés (`null`).
pub fn runtime_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Int | TypeRef::Float => "Number".to_owned(),
        TypeRef::Bool => "Boolean".to_owned(),
        TypeRef::Str | TypeRef::Bytes => "String".to_owned(),
        TypeRef::List(_) | TypeRef::Iter(_) => "Array".to_owned(),
        TypeRef::Map(..) => "Object".to_owned(),
        TypeRef::Opt(inner) => runtime_type(inner),
        TypeRef::Void | TypeRef::Mixed => "null".to_owned(),
        TypeRef::Union(items) => {
            let types: Vec<String> = items.iter().map(runtime_type).collect();
            if types.iter().any(|t| t == "null") {
                "null".to_owned()
            } else {
                format!("[{}]", types.join(", "))
            }
        }
        TypeRef::Named(name) => match name.as_str() {
            "string" | "String" => "String",
            "number" | "Number" => "Number",
            "boolean" | "Boolean" => "Boolean",
            "Array" => "Array",
            "Function" => "Function",
            "Date" => "Date",
            "any" | "unknown" => "null",
            _ => "Object",
        }
        .to_owned(),
    }
}

/// Az érték nélküli `ref` / `form`-mező értelmes alapértéke a típusból.
pub fn default_literal(ty: &TypeRef) -> Option<&'static str> {
    match ty {
        TypeRef::Int | TypeRef::Float => Some("0"),
        TypeRef::Str | TypeRef::Bytes => Some("''"),
        TypeRef::Bool => Some("false"),
        TypeRef::List(_) => Some("[]"),
        TypeRef::Map(..) => Some("{}"),
        TypeRef::Opt(_) => Some("null"),
        TypeRef::Named(n) => match n.as_str() {
            "string" => Some("''"),
            "number" => Some("0"),
            "boolean" => Some("false"),
            _ => None,
        },
        _ => None,
    }
}
