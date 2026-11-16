//! Típus-leképezés: a nyelvfüggetlen `TypeRef` TypeScript-típussá, Vue futásidejű prop-típussá (`String`, `Array` …)
//! és értelmes alapértékké (`ref<number>(0)`).

use super::model::TypeField;
use crate::generate::ast::TypeRef;
use crate::generate::error::{GenError, GenResult};
use crate::generate::head::{is_ident, matching, split_once_top, split_top};

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
            object if object.starts_with('{') => Some("{}"),
            _ => None,
        },
        _ => None,
    }
}

fn bad_type(text: &str) -> GenError {
    GenError::new(crate::tf!("bad type: {}", "hibás típus: {}", text))
}

/// Típus-kifejezés értelmezése; a `TypeRef::parse`-on túl a TS objektum-típus literált is érti (`{ flag: str; on?: bool }`),
/// akár `list<…>` / `opt<…>` / `map<…>` / `?` / `|` belsejében is. Az objektum `Named` típus lesz, a kész TS-szöveggel.
pub fn parse_type(text: &str) -> GenResult<TypeRef> {
    let text = text.trim();
    if !text.contains('{') {
        return TypeRef::parse(text);
    }
    if let Some(inner) = text.strip_prefix('?') {
        return Ok(TypeRef::Opt(Box::new(parse_type(inner)?)));
    }
    let parts = split_top(text, '|');
    if parts.len() > 1 {
        let members = parts
            .iter()
            .map(|p| parse_type(p))
            .collect::<GenResult<Vec<_>>>()?;
        return Ok(TypeRef::Union(members));
    }
    if text.starts_with('{') {
        if matching(text, '{', '}') != Some(text.len() - 1) {
            return Err(bad_type(text));
        }
        let fields = parse_fields(&text[1..text.len() - 1])?;
        return Ok(TypeRef::Named(object_ts(&fields, "; ")));
    }
    let open = text.find('<').ok_or_else(|| bad_type(text))?;
    let inner = text[open + 1..]
        .strip_suffix('>')
        .ok_or_else(|| bad_type(text))?;
    let mut args = split_top(inner, ',')
        .iter()
        .map(|a| parse_type(a))
        .collect::<GenResult<Vec<_>>>()?
        .into_iter();
    match (text[..open].trim(), args.next(), args.next(), args.next()) {
        ("list", Some(t), None, _) => Ok(TypeRef::List(Box::new(t))),
        ("map", Some(k), Some(v), None) => Ok(TypeRef::Map(Box::new(k), Box::new(v))),
        ("opt", Some(t), None, _) => Ok(TypeRef::Opt(Box::new(t))),
        ("iter", Some(t), None, _) => Ok(TypeRef::Iter(Box::new(t))),
        _ => Err(bad_type(text)),
    }
}

/// Egy objektum-típus mezői: `flag:str,on?:bool` (vessző, pontosvessző vagy újsor választja el őket).
pub fn parse_fields(inner: &str) -> GenResult<Vec<TypeField>> {
    let mut pieces = Vec::new();
    for by_comma in split_top(inner, ',') {
        for by_semi in split_top(&by_comma, ';') {
            pieces.extend(by_semi.lines().map(str::to_owned));
        }
    }
    pieces
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|piece| {
            let (name, ty) = split_once_top(piece, ':').ok_or_else(|| bad_type(piece))?;
            let name = name.trim();
            let (name, optional) = match name.strip_suffix('?') {
                Some(n) => (n.trim(), true),
                None => (name, false),
            };
            if !is_ident(name) {
                return Err(GenError::new(crate::tf!(
                    "bad name: `{}` (letters, digits, `_`)",
                    "hibás név: `{}` (betű, számjegy, `_`)",
                    name
                )));
            }
            Ok(TypeField {
                name: name.to_owned(),
                optional,
                ty: parse_type(ty)?,
            })
        })
        .collect()
}

/// A mezők TS objektum-típusként: `{ flag: string; on?: boolean }` (`sep` a mezők közt).
pub fn object_ts(fields: &[TypeField], sep: &str) -> String {
    if fields.is_empty() {
        return "{}".to_owned();
    }
    let body: Vec<String> = fields
        .iter()
        .map(|f| {
            format!(
                "{}{}: {}",
                f.name,
                if f.optional { "?" } else { "" },
                ts_type(&f.ty)
            )
        })
        .collect();
    format!("{{ {} }}", body.join(sep))
}
