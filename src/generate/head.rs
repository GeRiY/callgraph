//! Az elem-fejek értelmezése (az Interpreter minta második lépése): a tokenizáló egy-egy szavából típus-deklaráció,
//! tag, paraméter-lista, `ns:` / `use:` direktíva. A szintaxis röviden:
//!
//! - típus: `c:Név` osztály, `i:` interfész, `t:` trait, `e:` enum, `s:` struct; módosítók a fajta betű után:
//!   `!` final, `~` abstract, `%` readonly (`c!%:Név`); `<Ős>` extends, `[A,B]` implements;
//!   enum: `e:Status:string(Draft=draft,Paid=paid)` (vagy tiszta: `e:Dir(N,S)`)
//! - tag: láthatóság `+` public / `-` private / `#` protected, majd módosítók `~` abstract, `!` final, `%` readonly,
//!   `::` static; `név(paraméterek):Típus` metódus, `ctor(…)` konstruktor, `$név:Típus=érték` mező,
//!   `K=érték` / `K:Típus=érték` konstans
//! - paraméter: `[+|-|#][readonly ]Típus [&][...]$név[=alapérték]` (láthatóság-előtag = promoted property)

use super::ast::{
    EnumCase, Member, MemberKind, Param, TypeDecl, TypeKind, TypeRef, UseImport, Visibility,
};
use super::error::{GenError, GenResult};

/// A legfelső szinten (zárójelen, idézőjelen, `<…>`-on kívül) előforduló `sep` karakterek bájt-pozíciói.
fn top_level_indices(text: &str, sep: char) -> Vec<usize> {
    let mut found = Vec::new();
    let (mut depth, mut angle) = (0i32, 0i32);
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut prev = ' ';
    for (i, c) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            prev = c;
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '<' if prev.is_alphanumeric() || prev == '_' => angle += 1,
            '>' if angle > 0 && prev != '=' => angle -= 1,
            c if c == sep && depth <= 0 && angle <= 0 => found.push(i),
            _ => {}
        }
        prev = c;
    }
    found
}

/// Darabolás a legfelső szintű `sep` mentén (a darabok nincsenek vágva).
pub fn split_top(text: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut from = 0;
    for i in top_level_indices(text, sep) {
        parts.push(text[from..i].to_owned());
        from = i + sep.len_utf8();
    }
    parts.push(text[from..].to_owned());
    parts
}

/// Az első legfelső szintű `sep` két oldala.
pub fn split_once_top(text: &str, sep: char) -> Option<(&str, &str)> {
    let i = *top_level_indices(text, sep).first()?;
    Some((&text[..i], &text[i + sep.len_utf8()..]))
}

/// A `open`-nel kezdődő szöveg párjának bájt-pozíciója (idézőjelek között nem számol).
pub fn matching(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

pub fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn bad(en_hu: String) -> GenError {
    GenError::new(en_hu)
}

fn bad_name(name: &str) -> GenError {
    bad(crate::tf!(
        "bad name: `{}` (letters, digits, `_`)",
        "hibás név: `{}` (betű, számjegy, `_`)",
        name
    ))
}

/// `ns:Some\Ns`.
pub fn parse_ns(text: &str) -> GenResult<String> {
    let ns = text.trim().trim_matches('\\');
    if ns.is_empty()
        || !ns
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '\\')
    {
        return Err(bad(crate::tf!(
            "bad namespace: `{}` (e.g. `ns:App\\Services`)",
            "hibás névtér: `{}` (pl. `ns:App\\Services`)",
            text
        )));
    }
    Ok(ns.to_owned())
}

/// `use:Foo\Bar,Baz\Qux as Q`.
pub fn parse_uses(text: &str) -> GenResult<Vec<UseImport>> {
    let mut out = Vec::new();
    for piece in split_top(text, ',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let (path, alias) = match piece.split_once(" as ") {
            Some((p, a)) => (p.trim(), Some(a.trim())),
            None => (piece, None),
        };
        let fqcn = path.trim_start_matches('\\');
        if fqcn.is_empty()
            || !fqcn
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '\\')
            || alias.is_some_and(|a| !is_ident(a))
        {
            return Err(bad(crate::tf!(
                "bad import: `{}` (e.g. `use:App\\Models\\Order,Acme\\Client as C`)",
                "hibás import: `{}` (pl. `use:App\\Models\\Order,Acme\\Client as C`)",
                piece
            )));
        }
        out.push(UseImport {
            fqcn: fqcn.to_owned(),
            alias: alias.map(str::to_owned),
        });
    }
    Ok(out)
}

/// Típus-deklaráció feje-e (`c:`, `i:`, `t:`, `e:`, `s:`, módosítókkal: `c!%:`).
pub fn is_decl_head(head: &str) -> bool {
    let Some((prefix, rest)) = head.split_once(':') else {
        return false;
    };
    let mut chars = prefix.chars();
    matches!(chars.next(), Some('c' | 'i' | 't' | 'e' | 's'))
        && chars.all(|c| matches!(c, '!' | '~' | '%'))
        && rest
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
}

/// Típus-deklaráció feje (a tagok nélkül); `list`: a külön álló `[A, B]` (implements).
pub fn parse_decl(head: &str, list: Option<&str>) -> GenResult<TypeDecl> {
    let Some((prefix, rest)) = head.split_once(':') else {
        return Err(bad(crate::t!("bad type head", "hibás típus-fej").to_owned()));
    };
    let kind = match prefix.chars().next() {
        Some('c') => TypeKind::Class,
        Some('i') => TypeKind::Interface,
        Some('t') => TypeKind::Trait,
        Some('e') => TypeKind::Enum,
        _ => TypeKind::Struct,
    };
    let name_end = rest.find(['<', '[', ':', '(']).unwrap_or(rest.len());
    let name = rest[..name_end].trim();
    if !is_ident(name) {
        return Err(bad_name(name));
    }
    let mut decl = TypeDecl::new(kind, name);
    for modifier in prefix.chars().skip(1) {
        match modifier {
            '!' => decl.is_final = true,
            '~' => decl.is_abstract = true,
            _ => decl.is_readonly = true,
        }
    }
    let mut tail = rest[name_end..].trim_start();
    while let Some(first) = tail.chars().next() {
        match first {
            '<' => {
                let end = matching(tail, '<', '>').ok_or_else(|| unclosed('<'))?;
                decl.extends = type_list(&tail[1..end])?;
                tail = tail[end + 1..].trim_start();
            }
            '[' => {
                let end = matching(tail, '[', ']').ok_or_else(|| unclosed('['))?;
                decl.implements.extend(type_list(&tail[1..end])?);
                tail = tail[end + 1..].trim_start();
            }
            ':' => {
                let end = tail.find('(').unwrap_or(tail.len());
                decl.backing = Some(TypeRef::parse(&tail[1..end])?);
                tail = tail[end..].trim_start();
            }
            '(' => {
                let end = matching(tail, '(', ')').ok_or_else(|| unclosed('('))?;
                decl.cases = parse_cases(&tail[1..end])?;
                tail = tail[end + 1..].trim_start();
            }
            other => {
                return Err(bad(crate::tf!(
                    "unexpected `{}` in the type head `{}` (e.g. `c!:Pay<Base>[Chargeable]`)",
                    "váratlan `{}` a típus-fejben: `{}` (pl. `c!:Pay<Base>[Chargeable]`)",
                    other,
                    head
                )));
            }
        }
    }
    if let Some(items) = list {
        decl.implements.extend(type_list(items)?);
    }
    // interfésznél az `[A]` és a `<A>` egyaránt az `extends` lista
    if decl.kind == TypeKind::Interface {
        let implemented = std::mem::take(&mut decl.implements);
        decl.extends.extend(implemented);
    }
    if decl.extends.len() > 1 && decl.kind != TypeKind::Interface {
        return Err(bad(crate::t!(
            "a type extends one base only: `<Base>`; more interfaces go to `[A,B]`",
            "egy típusnak egy őse lehet: `<Ős>`; több interfész a `[A,B]`-be kerül"
        )
        .to_owned()));
    }
    if decl.kind == TypeKind::Struct {
        decl.is_final = true;
    }
    Ok(decl)
}

fn unclosed(open: char) -> GenError {
    bad(crate::tf!("unclosed `{}`", "lezáratlan `{}`", open))
}

fn type_list(text: &str) -> GenResult<Vec<TypeRef>> {
    split_top(text, ',')
        .iter()
        .filter(|p| !p.trim().is_empty())
        .map(|p| TypeRef::parse(p))
        .collect()
}

fn parse_cases(text: &str) -> GenResult<Vec<EnumCase>> {
    let mut cases = Vec::new();
    for piece in split_top(text, ',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let (name, value) = match piece.split_once('=') {
            Some((n, v)) => (n.trim(), Some(v.trim().to_owned())),
            None => (piece, None),
        };
        if !is_ident(name) {
            return Err(bad_name(name));
        }
        cases.push(EnumCase {
            name: name.to_owned(),
            value,
        });
    }
    Ok(cases)
}

fn visibility_of(c: Option<char>) -> Option<Visibility> {
    match c {
        Some('+') => Some(Visibility::Public),
        Some('-') => Some(Visibility::Private),
        Some('#') => Some(Visibility::Protected),
        _ => None,
    }
}

/// Egy tag feje; `raw`: a `{…}` törzs.
pub fn parse_member(head: &str, raw: Option<&str>) -> GenResult<Member> {
    let mut s = head;
    let visibility = visibility_of(s.chars().next());
    if visibility.is_some() {
        s = &s[1..];
    }
    let (mut is_abstract, mut is_final, mut is_readonly, mut is_static) =
        (false, false, false, false);
    loop {
        if let Some(rest) = s.strip_prefix("::") {
            is_static = true;
            s = rest;
        } else if let Some(rest) = s.strip_prefix('~') {
            is_abstract = true;
            s = rest;
        } else if let Some(rest) = s.strip_prefix('!') {
            is_final = true;
            s = rest;
        } else if let Some(rest) = s.strip_prefix('%') {
            is_readonly = true;
            s = rest;
        } else {
            break;
        }
    }
    let mut member = if let Some(rest) = s.strip_prefix('$') {
        parse_field(rest)?
    } else {
        parse_callable_or_const(s, head)?
    };
    if raw.is_some() && !matches!(member.kind, MemberKind::Method | MemberKind::Ctor) {
        return Err(bad(crate::t!(
            "only methods and ctor take a {body}",
            "csak metódus és ctor kaphat {törzset}"
        )
        .to_owned()));
    }
    member.visibility = visibility;
    member.is_abstract = is_abstract;
    member.is_final = is_final;
    member.is_readonly = is_readonly;
    member.is_static = is_static;
    member.body = raw.map(str::to_owned);
    Ok(member)
}

/// A `$` utáni rész: `név[:Típus][=érték]`.
fn parse_field(rest: &str) -> GenResult<Member> {
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if !is_ident(name) {
        return Err(bad_name(name));
    }
    let tail = rest[end..].trim();
    let (ty_part, default) = match split_once_top(tail, '=') {
        Some((a, b)) => (a.trim(), Some(b.trim().to_owned())),
        None => (tail, None),
    };
    let mut member = Member::new(MemberKind::Field, name);
    if let Some(ty) = ty_part.strip_prefix(':') {
        member.ty = Some(TypeRef::parse(ty)?);
    } else if !ty_part.is_empty() {
        return Err(bad(crate::tf!(
            "bad property `${}{}` (e.g. `$name:string=''`)",
            "hibás property: `${}{}` (pl. `$name:string=''`)",
            name,
            ty_part
        )));
    }
    member.default = default.filter(|d| !d.is_empty());
    Ok(member)
}

/// `név(paraméterek)[:Típus]` metódus / ctor, vagy `K[:Típus]=érték` konstans.
fn parse_callable_or_const(s: &str, head: &str) -> GenResult<Member> {
    let end = s
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    let name = &s[..end];
    let tail = s[end..].trim_start();
    if !is_ident(name) {
        return Err(bad(crate::tf!(
            "unknown element `{}`: write a method `name():type`, a property `$name:type` or a constant `NAME=1`",
            "ismeretlen elem: `{}`: metódus `név():típus`, property `$név:típus` vagy konstans `NÉV=1` lehet",
            head
        )));
    }
    if tail.starts_with('(') {
        let close = matching(tail, '(', ')').ok_or_else(|| unclosed('('))?;
        let rest = tail[close + 1..].trim();
        let is_ctor = matches!(name, "ctor" | "__construct");
        let mut member = Member::new(
            if is_ctor {
                MemberKind::Ctor
            } else {
                MemberKind::Method
            },
            name,
        );
        member.params = parse_params(&tail[1..close])?;
        if let Some(ty) = rest.strip_prefix(':') {
            member.ty = Some(TypeRef::parse(ty)?);
        } else if !rest.is_empty() {
            return Err(bad(crate::tf!(
                "unexpected `{}` after the parameters of `{}` (return type: `name():Type`)",
                "váratlan `{}` a(z) `{}` paraméterei után (visszatérési típus: `név():Típus`)",
                rest,
                name
            )));
        }
        return Ok(member);
    }
    let Some((ty_part, value)) = split_once_top(tail, '=') else {
        return Err(bad(crate::tf!(
            "unknown element `{}`: write a method `name():type`, a property `$name:type` or a constant `NAME=1`",
            "ismeretlen elem: `{}`: metódus `név():típus`, property `$név:típus` vagy konstans `NÉV=1` lehet",
            head
        )));
    };
    let mut member = Member::new(MemberKind::Const, name);
    let ty_part = ty_part.trim();
    if let Some(ty) = ty_part.strip_prefix(':') {
        member.ty = Some(TypeRef::parse(ty)?);
    } else if !ty_part.is_empty() {
        return Err(bad_name(ty_part));
    }
    member.default = Some(value.trim().to_owned());
    Ok(member)
}

/// A zárójelek közti paraméter-lista.
pub fn parse_params(text: &str) -> GenResult<Vec<Param>> {
    let mut out = Vec::new();
    for piece in split_top(text, ',') {
        let piece = piece.trim();
        if !piece.is_empty() {
            out.push(parse_param(piece)?);
        }
    }
    Ok(out)
}

fn parse_param(piece: &str) -> GenResult<Param> {
    let mut s = piece;
    let promote = visibility_of(s.chars().next());
    if promote.is_some() {
        s = s[1..].trim_start();
    }
    let mut readonly = false;
    while let Some(rest) = s
        .strip_prefix("readonly ")
        .or_else(|| s.strip_prefix("ro "))
    {
        readonly = true;
        s = rest.trim_start();
    }
    // nyelvfüggetlen alak: a név `$` nélkül is jó (`Request request`, `int retries=3`, `...xs`) — az utolsó szó a
    // név, előtte a típus; a PHP-emitter teszi elé a `$`-t
    let normalized;
    if !s.contains('$') {
        let (decl, default) = match s.split_once('=') {
            Some((d, rest)) => (d.trim_end(), Some(rest)),
            None => (s.trim_end(), None),
        };
        let (ty, name) = match decl.rfind(char::is_whitespace) {
            Some(at) => (&decl[..at], decl[at..].trim_start()),
            None => ("", decl),
        };
        let bare = name.trim_start_matches(['.', '&']);
        let prefix = &name[..name.len() - bare.len()];
        normalized = match default {
            Some(d) => format!("{ty} {prefix}${bare}={d}"),
            None => format!("{ty} {prefix}${bare}"),
        };
        s = normalized.as_str();
    }
    let Some(dollar) = s.find('$') else {
        return Err(bad(crate::tf!(
            "parameter `{}` needs a `$name` (e.g. `Order $order`)",
            "a(z) `{}` paraméterhez `$név` kell (pl. `Order $order`)",
            piece
        )));
    };
    let mut before = s[..dollar].trim_end();
    let (mut variadic, mut by_ref) = (false, false);
    loop {
        if let Some(b) = before.strip_suffix("...") {
            variadic = true;
            before = b.trim_end();
        } else if let Some(b) = before.strip_suffix('&') {
            by_ref = true;
            before = b.trim_end();
        } else {
            break;
        }
    }
    let ty = if before.is_empty() {
        None
    } else {
        Some(TypeRef::parse(before)?)
    };
    let after = &s[dollar + 1..];
    let (name, default) = match after.split_once('=') {
        Some((n, d)) => (
            n.trim(),
            Some(d.trim().to_owned()).filter(|d| !d.is_empty()),
        ),
        None => (after.trim(), None),
    };
    if !is_ident(name) {
        return Err(bad_name(name));
    }
    Ok(Param {
        name: name.to_owned(),
        ty,
        default,
        variadic,
        by_ref,
        promote,
        readonly,
    })
}
