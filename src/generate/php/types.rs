//! PHP-típusleképezés és névgyűjtés: a nyelvfüggetlen `TypeRef` PHP-típussá és docblock-típussá alakítása, valamint
//! az importra szoruló osztály-nevek kigyűjtése (típusokból és nyers törzsekből).

use std::collections::BTreeSet;

use crate::generate::ast::{Member, TypeRef};

/// A natív PHP-típus: `list<T>` / `map<K,V>` → `array`, `str` → `string`, `opt<T>` → `?T`, `iter<T>` → `iterable`.
pub fn php_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Int => "int".to_owned(),
        TypeRef::Float => "float".to_owned(),
        TypeRef::Bool => "bool".to_owned(),
        TypeRef::Str | TypeRef::Bytes => "string".to_owned(),
        TypeRef::Void => "void".to_owned(),
        TypeRef::Mixed => "mixed".to_owned(),
        TypeRef::List(_) | TypeRef::Map(..) => "array".to_owned(),
        TypeRef::Iter(_) => "iterable".to_owned(),
        TypeRef::Opt(inner) => match &**inner {
            TypeRef::Union(_) => format!("{}|null", php_type(inner)),
            TypeRef::Mixed => "mixed".to_owned(),
            TypeRef::Opt(_) => php_type(inner),
            other => format!("?{}", php_type(other)),
        },
        TypeRef::Union(items) => items.iter().map(php_type).collect::<Vec<_>>().join("|"),
        TypeRef::Named(name) => name.clone(),
    }
}

/// A docblock-típus (`@param list<Order> $orders`, `array<string,int>`, `iterable<T>`, `T|null`).
pub fn doc_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::List(t) => format!("list<{}>", doc_type(t)),
        TypeRef::Map(k, v) => format!("array<{},{}>", doc_type(k), doc_type(v)),
        TypeRef::Iter(t) => format!("iterable<{}>", doc_type(t)),
        TypeRef::Opt(t) => format!("{}|null", doc_type(t)),
        TypeRef::Union(items) => items.iter().map(doc_type).collect::<Vec<_>>().join("|"),
        other => php_type(other),
    }
}

/// Kell-e docblock a típushoz (a natív típus nem fejezi ki a tartalmát).
pub fn needs_doc(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::List(_) | TypeRef::Map(..) => true,
        TypeRef::Iter(t) => **t != TypeRef::Mixed,
        TypeRef::Opt(t) => needs_doc(t),
        TypeRef::Union(items) => items.iter().any(needs_doc),
        _ => false,
    }
}

/// Generátor-jellegű visszatérési típus (`Generator`, `iterable`, `iter<T>`): törzs nélkül `yield from [];` kap.
pub fn is_generator_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Iter(_) => true,
        TypeRef::Named(name) => matches!(name.trim_start_matches('\\'), "Generator" | "iterable"),
        _ => false,
    }
}

/// Beépített típus-név (nem osztály): soha nem importáljuk.
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "int"
            | "float"
            | "bool"
            | "string"
            | "array"
            | "object"
            | "callable"
            | "iterable"
            | "mixed"
            | "void"
            | "null"
            | "never"
            | "self"
            | "static"
            | "parent"
            | "false"
            | "true"
            | "resource"
    )
}

/// Importálandó jelölt-e a név: rövid (nincs `\`), nagybetűvel kezdődik, nem beépített.
pub fn is_candidate(name: &str) -> bool {
    !name.contains('\\') && name.chars().next().is_some_and(char::is_uppercase) && !is_builtin(name)
}

/// A típusban hivatkozott jelölt-nevek.
pub fn type_names(ty: &TypeRef, out: &mut BTreeSet<String>) {
    let mut names = Vec::new();
    ty.names(&mut names);
    out.extend(names.into_iter().filter(|n| is_candidate(n)));
}

/// A tag minden hivatkozott jelölt-neve: típusok, paraméterek, alapértékek, nyers törzs.
pub fn member_names(member: &Member, out: &mut BTreeSet<String>) {
    if let Some(ty) = &member.ty {
        type_names(ty, out);
    }
    for param in &member.params {
        if let Some(ty) = &param.ty {
            type_names(ty, out);
        }
        if let Some(default) = &param.default {
            out.extend(scan_names(default));
        }
    }
    if let Some(default) = &member.default {
        out.extend(scan_names(default));
    }
    if let Some(body) = &member.body {
        out.extend(scan_names(body));
    }
}

fn skip_quoted(chars: &[char], from: usize) -> usize {
    let quote = chars[from];
    let mut i = from + 1;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    chars.len()
}

/// PHP-kódrészlet osztály-hivatkozásai: `new X`, `X::`, `instanceof X`, `extends X`, típusos változó (`X $x`,
/// `catch (X|Y $e)`); a sztringeket és a megjegyzéseket átugorja, a `\`-t tartalmazó (minősített) neveket nem vizsgálja.
pub fn scan_names(code: &str) -> Vec<String> {
    let chars: Vec<char> = code.chars().collect();
    let mut found = Vec::new();
    let mut prev_word = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '\'' || c == '"' {
            i = skip_quoted(&chars, i);
            continue;
        }
        if (c == '/' && next == Some('/')) || (c == '#' && next != Some('[')) {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && next == Some('*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '\\' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '\\')
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let before = start.checked_sub(1).map(|p| chars[p]);
            let accessed = matches!(before, Some('$' | '>' | ':'));
            let mut j = i;
            while chars.get(j).is_some_and(|c| c.is_whitespace()) {
                j += 1;
            }
            let after = chars.get(j).copied();
            let static_use = after == Some(':') && chars.get(j + 1) == Some(&':');
            let typed_var = after == Some('$');
            let union_member = after == Some('|') && word.chars().any(char::is_lowercase);
            let keyword = matches!(
                prev_word.as_str(),
                "new" | "instanceof" | "extends" | "implements"
            );
            if !accessed
                && is_candidate(&word)
                && (keyword || static_use || typed_var || union_member)
            {
                found.push(word.clone());
            }
            prev_word = word;
            continue;
        }
        if !c.is_whitespace() {
            prev_word.clear();
        }
        i += 1;
    }
    found
}
