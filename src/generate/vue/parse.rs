//! A Vue-nyelvjárású kifejezés értelmezése: a közös fa (`parser::parse_tree`) elemeiből `Program`
//! (script-tagok, template-elemek, stílus-szabályok). A fej-szintaxis a modul-dokumentációban (`vue/mod.rs`).

use super::model::{
    Attr, Element, Emit, FormField, Import, Kind, Member, Program, Prop, Rule, Script, Style,
    VParam,
};
use crate::generate::ast::TypeRef;
use crate::generate::error::{GenError, GenResult};
use crate::generate::head::{is_ident, matching, split_once_top, split_top};
use crate::generate::parser::{Expanded, parse_tree};

fn located(node: &Expanded, e: &GenError) -> GenError {
    GenError::new(crate::tf!(
        "{} (col {}: `{}`)",
        "{} ({}. oszlop: `{}`)",
        e.message(),
        node.col,
        node.head
    ))
}

fn bad_name(name: &str) -> GenError {
    GenError::new(crate::tf!(
        "bad name: `{}` (letters, digits, `_`)",
        "hibás név: `{}` (betű, számjegy, `_`)",
        name
    ))
}

fn ident(name: &str) -> GenResult<String> {
    let name = name.trim();
    if is_ident(name) {
        Ok(name.to_owned())
    } else {
        Err(bad_name(name))
    }
}

fn no_children(node: &Expanded) -> GenResult<()> {
    if node.children.is_empty() {
        Ok(())
    } else {
        Err(GenError::new(crate::tf!(
            "`{}` takes no children (`>`)",
            "a(z) `{}` nem kaphat gyerekeket (`>`)",
            node.head
        )))
    }
}

/// A kifejezés értelmezése.
pub fn parse(source: &str) -> GenResult<Program> {
    let tree = parse_tree(source)?;
    let mut program = Program::default();
    let mut loose = Script::default();
    let mut has_loose = false;
    for node in &tree {
        top(node, &mut program, &mut loose, &mut has_loose).map_err(|e| located(node, &e))?;
    }
    if has_loose {
        let script = program.script.get_or_insert_with(Script::default);
        script.members.append(&mut loose.members);
        script.imports.append(&mut loose.imports);
    }
    if program.sections() == 0 {
        return Err(GenError::new(
            crate::t!(
                "nothing to generate: the expression has no script / template / style",
                "nincs mit generálni: a kifejezésben nincs script / template / style"
            )
            .to_owned(),
        ));
    }
    Ok(program)
}

fn is_style_head(head: &str) -> bool {
    matches!(head, "style" | "style!" | "style:scoped" | "style:global")
}

fn top(
    node: &Expanded,
    program: &mut Program,
    loose: &mut Script,
    has_loose: &mut bool,
) -> GenResult<()> {
    let head = node.head.as_str();
    if let Some(name) = head.strip_prefix("vue:") {
        program.kind = Kind::Sfc;
        program.name = Some(ident(name)?);
        for child in &node.children {
            section(child, program).map_err(|e| located(child, &e))?;
        }
    } else if let Some(name) = head.strip_prefix("comp:") {
        program.kind = Kind::Composable;
        program.name = Some(ident(name)?);
        let script = program.script.get_or_insert_with(Script::default);
        for child in &node.children {
            script_item(child, script).map_err(|e| located(child, &e))?;
        }
    } else if head == "script" || head == "template" || is_style_head(head) {
        section(node, program)?;
    } else {
        script_item(node, loose)?;
        *has_loose = true;
    }
    Ok(())
}

fn section(node: &Expanded, program: &mut Program) -> GenResult<()> {
    let head = node.head.as_str();
    if node.raw.is_some() || node.list.is_some() {
        return Err(GenError::new(
            crate::t!(
                "a section has no `{…}` / `[…]`: its content follows after `>`",
                "a szakasznak nincs `{…}` / `[…]` része: a tartalma a `>` után jön"
            )
            .to_owned(),
        ));
    }
    match head {
        "script" => {
            let script = program.script.get_or_insert_with(Script::default);
            for child in &node.children {
                script_item(child, script).map_err(|e| located(child, &e))?;
            }
        }
        "template" => {
            let mut elements = program.template.take().unwrap_or_default();
            for child in &node.children {
                elements.push(element(child).map_err(|e| located(child, &e))?);
            }
            program.template = Some(elements);
        }
        h if is_style_head(h) => {
            let scoped = h != "style:global";
            let style = program.style.get_or_insert_with(|| Style {
                scoped,
                rules: Vec::new(),
            });
            for child in &node.children {
                style
                    .rules
                    .push(rule(child).map_err(|e| located(child, &e))?);
            }
        }
        other => {
            return Err(GenError::new(crate::tf!(
                "expected script / template / style under vue:, found `{}`",
                "a vue: alatt script / template / style kell, de ez jött: `{}`",
                other
            )));
        }
    }
    Ok(())
}

fn script_item(node: &Expanded, script: &mut Script) -> GenResult<()> {
    no_children(node)?;
    if let Some(rest) = node.head.strip_prefix("use:") {
        script.imports.extend(parse_uses(rest)?);
        return Ok(());
    }
    script.members.push(member(node)?);
    Ok(())
}

/// `kw(…)` alak: a zárójelek közti szöveg.
fn call_args<'a>(head: &'a str, kw: &str) -> Option<&'a str> {
    head.strip_prefix(kw)?.strip_prefix('(')?.strip_suffix(')')
}

fn member(node: &Expanded) -> GenResult<Member> {
    let head = node.head.as_str();
    let raw = node.raw.as_deref();
    if node.list.is_some() {
        return Err(GenError::new(
            crate::t!(
                "`[…]` belongs to a template element",
                "a `[…]` template-elemhez tartozik"
            )
            .to_owned(),
        ));
    }
    if let Some(inner) = call_args(head, "props") {
        return Ok(Member::Props(parse_props(inner)?));
    }
    if let Some(inner) = call_args(head, "emits") {
        return Ok(Member::Emits(parse_emits(inner)?));
    }
    if head == "form" || head.starts_with("form(") || head.starts_with("form:") {
        return parse_form(&head["form".len()..]);
    }
    if let Some(rest) = head.strip_prefix("ref:") {
        let (name, ty, init) = decl(rest, raw)?;
        return Ok(Member::Ref { name, ty, init });
    }
    if let Some(rest) = head.strip_prefix("computed:") {
        let (name, ty, _) = decl(rest, None)?;
        return Ok(Member::Computed {
            name,
            ty,
            body: raw.map(str::to_owned),
        });
    }
    for (prefix, mutable) in [("const:", false), ("let:", true)] {
        if let Some(rest) = head.strip_prefix(prefix) {
            let (name, ty, value) = decl(rest, raw)?;
            return Ok(Member::Const {
                name,
                ty,
                value,
                mutable,
            });
        }
    }
    for (prefix, is_async) in [("fn:", false), ("async:", true)] {
        if let Some(rest) = head.strip_prefix(prefix) {
            return func(rest, raw, is_async);
        }
    }
    if let Some(name) = head.strip_prefix("on:") {
        return Ok(Member::Hook {
            name: ident(name)?,
            body: raw.map(str::to_owned),
        });
    }
    Err(GenError::new(crate::tf!(
        "unknown script member `{}` (props(…) emits(…) ref:n:T=v computed:n:T{{…}} fn:n(p:T):R{{…}} form(…) const:n:T=v on:mounted{{…}} use:path)",
        "ismeretlen script-tag: `{}` (props(…) emits(…) ref:n:T=v computed:n:T{{…}} fn:n(p:T):R{{…}} form(…) const:n:T=v on:mounted{{…}} use:út)",
        head
    )))
}

/// `név:típus=érték` → (név, típus, érték); az érték hiányában a `{…}` törzs.
fn decl(rest: &str, raw: Option<&str>) -> GenResult<(String, Option<TypeRef>, Option<String>)> {
    let (lhs, init) = match split_once_top(rest, '=') {
        Some((l, r)) => (l, Some(r.trim().to_owned()).filter(|v| !v.is_empty())),
        None => (rest, None),
    };
    let init = init.or_else(|| raw.map(str::to_owned));
    let (name, ty) = name_type(lhs)?;
    Ok((name, ty, init))
}

fn name_type(text: &str) -> GenResult<(String, Option<TypeRef>)> {
    match split_once_top(text, ':') {
        Some((name, ty)) => Ok((ident(name)?, Some(TypeRef::parse(ty)?))),
        None => Ok((ident(text)?, None)),
    }
}

fn func(rest: &str, raw: Option<&str>, is_async: bool) -> GenResult<Member> {
    let unclosed = || GenError::new(crate::t!("unclosed `(`", "lezáratlan `(`").to_owned());
    let (name, params, tail) = match rest.find('(') {
        Some(open) => {
            let close = open + matching(&rest[open..], '(', ')').ok_or_else(unclosed)?;
            (
                ident(&rest[..open])?,
                parse_params(&rest[open + 1..close])?,
                &rest[close + 1..],
            )
        }
        None => match rest.split_once(':') {
            Some((name, _)) => (ident(name)?, Vec::new(), &rest[name.len()..]),
            None => (ident(rest)?, Vec::new(), ""),
        },
    };
    let tail = tail.trim();
    let ret = match tail.strip_prefix(':') {
        Some(ty) => Some(TypeRef::parse(ty)?),
        None if tail.is_empty() => None,
        None => {
            return Err(GenError::new(crate::tf!(
                "bad function head after `)`: `{}` (use `:Type`)",
                "hibás függvény-fej a `)` után: `{}` (`:Típus` kell)",
                tail
            )));
        }
    };
    Ok(Member::Func {
        name,
        params,
        ret,
        body: raw.map(str::to_owned),
        is_async,
    })
}

fn parse_params(text: &str) -> GenResult<Vec<VParam>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    split_top(text, ',')
        .iter()
        .map(|piece| {
            let piece = piece.trim();
            let (piece, rest) = match piece.strip_prefix("...") {
                Some(p) => (p, true),
                None => (piece, false),
            };
            let (lhs, default) = match split_once_top(piece, '=') {
                Some((l, d)) => (l, Some(d.trim().to_owned())),
                None => (piece, None),
            };
            let (name, ty) = match split_once_top(lhs, ':') {
                Some((n, t)) => (n.trim(), Some(TypeRef::parse(t)?)),
                None => (lhs.trim(), None),
            };
            let (name, optional) = match name.strip_suffix('?') {
                Some(n) => (n, true),
                None => (name, false),
            };
            Ok(VParam {
                name: ident(name)?,
                ty,
                default,
                optional,
                rest,
            })
        })
        .collect()
}

fn parse_props(inner: &str) -> GenResult<Vec<Prop>> {
    if inner.trim().is_empty() {
        return Err(GenError::new(
            crate::t!("props() is empty", "a props() üres").to_owned(),
        ));
    }
    split_top(inner, ',')
        .iter()
        .map(|piece| {
            let (lhs, default) = match split_once_top(piece, '=') {
                Some((l, d)) => (l, Some(d.trim().to_owned())),
                None => (piece.as_str(), None),
            };
            let (name, ty) = match split_once_top(lhs, ':') {
                Some((n, t)) => (n.trim(), Some(TypeRef::parse(t)?)),
                None => (lhs.trim(), None),
            };
            let (name, optional) = match name.strip_suffix('?') {
                Some(n) => (n, true),
                None => (name, false),
            };
            Ok(Prop {
                name: ident(name)?,
                ty,
                required: !optional && default.is_none(),
                default,
            })
        })
        .collect()
}

fn parse_emits(inner: &str) -> GenResult<Vec<Emit>> {
    if inner.trim().is_empty() {
        return Err(GenError::new(
            crate::t!("emits() is empty", "az emits() üres").to_owned(),
        ));
    }
    split_top(inner, ',')
        .iter()
        .map(|piece| {
            let (name, ty) = name_type(piece)?;
            Ok(Emit { name, payload: ty })
        })
        .collect()
}

/// `form(a,b:bool=true)` vagy `form:login(a,b)`: a `form` utáni szöveg.
fn parse_form(rest: &str) -> GenResult<Member> {
    let open = rest.find('(').ok_or_else(|| {
        GenError::new(
            crate::t!(
                "form needs its fields: `form(name,email)`",
                "a form-hoz mezők kellenek: `form(name,email)`"
            )
            .to_owned(),
        )
    })?;
    let name = match rest[..open].strip_prefix(':') {
        Some(n) => ident(n)?,
        None if open == 0 => "form".to_owned(),
        None => return Err(bad_name(&rest[..open])),
    };
    let inner = rest[open + 1..]
        .strip_suffix(')')
        .ok_or_else(|| GenError::new(crate::t!("unclosed `(`", "lezáratlan `(`").to_owned()))?;
    let fields = split_top(inner, ',')
        .iter()
        .filter(|p| !p.trim().is_empty())
        .map(|piece| {
            let (name, ty, default) = decl(piece, None)?;
            Ok(FormField { name, ty, default })
        })
        .collect::<GenResult<Vec<_>>>()?;
    if fields.is_empty() {
        return Err(GenError::new(
            crate::t!("form() has no fields", "a form()-nak nincs mezője").to_owned(),
        ));
    }
    Ok(Member::Form { name, fields })
}

/// `use:` lista: `@/Components/Card`, `@/Components/Card as C`, `vue(ref,computed)`.
fn parse_uses(rest: &str) -> GenResult<Vec<Import>> {
    let mut out = Vec::new();
    for item in split_top(rest, ',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if let Some(open) = item.find('(') {
            let inner = item[open + 1..].strip_suffix(')').ok_or_else(|| {
                GenError::new(crate::t!("unclosed `(`", "lezáratlan `(`").to_owned())
            })?;
            let named = inner.split(',').map(ident).collect::<GenResult<Vec<_>>>()?;
            out.push(Import {
                source: item[..open].trim().to_owned(),
                default: None,
                named,
            });
            continue;
        }
        let (path, alias) = match item.split_once(" as ") {
            Some((p, a)) => (p.trim(), Some(ident(a)?)),
            None => (item, None),
        };
        let file = path.rsplit('/').next().unwrap_or(path);
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        let default = match alias {
            Some(a) => a,
            None => ident(stem).map_err(|_| {
                GenError::new(crate::tf!(
                    "cannot name the import of `{}`: use `{} as Name`",
                    "nem tudom elnevezni a(z) `{}` importját: használd így: `{} as Név`",
                    path,
                    path
                ))
            })?,
        };
        let component = file.chars().next().is_some_and(char::is_uppercase) && !file.contains('.');
        let source = if component {
            format!("{path}.vue")
        } else {
            path.to_owned()
        };
        out.push(Import {
            source,
            default: Some(default),
            named: Vec::new(),
        });
    }
    Ok(out)
}

/// A számláló (`$`) cseréje a szövegben: csak az önálló `$` (nem `$emit`, nem `a$b`).
fn number_text(text: &str, counter: Option<u32>) -> String {
    let Some(n) = counter else {
        return text.to_owned();
    };
    let chars: Vec<char> = text.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' {
            let mut width = 0usize;
            while chars.get(i + width) == Some(&'$') {
                width += 1;
            }
            let before = i.checked_sub(1).map(|p| chars[p]);
            let after = chars.get(i + width).copied();
            if !before.is_some_and(word) && !after.is_some_and(word) {
                out.push_str(&format!("{n:0width$}"));
                i += width;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// A `{…}` szöveg: a teljes egészében `{ … }`-be zárt (mustache) szöveget még egy pár kapcsos zárójel veszi körül.
fn text_of(raw: &str, counter: Option<u32>) -> Option<String> {
    let text = number_text(raw, counter);
    if text.is_empty() {
        return None;
    }
    if text.starts_with('{') && matching(&text, '{', '}') == Some(text.len() - 1) {
        return Some(format!("{{{text}}}"));
    }
    Some(text)
}

fn element(node: &Expanded) -> GenResult<Element> {
    let (tag, id, classes) = emmet_head(&node.head)?;
    let attrs = match &node.list {
        Some(list) => parse_attrs(list)?,
        None => Vec::new(),
    };
    let children = node
        .children
        .iter()
        .map(|c| element(c).map_err(|e| located(c, &e)))
        .collect::<GenResult<Vec<_>>>()?;
    Ok(Element {
        tag,
        id,
        classes,
        attrs,
        text: node.raw.as_deref().and_then(|r| text_of(r, node.counter)),
        children,
    })
}

/// `tag.class1.class2#id` (Emmet-fej); a tag elmaradhat (`.card` → `div`).
fn emmet_head(head: &str) -> GenResult<(String, Option<String>, Vec<String>)> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Tag,
        Class,
        Id,
    }
    let mut tag = String::new();
    let mut id = None;
    let mut classes = Vec::new();
    let mut mode = Mode::Tag;
    let mut buffer = String::new();
    let mut flush = |mode: Mode, buffer: &mut String| -> GenResult<()> {
        let piece = std::mem::take(buffer);
        match mode {
            Mode::Tag => tag = piece,
            Mode::Class | Mode::Id if piece.is_empty() => {
                return Err(GenError::new(crate::tf!(
                    "empty class / id in `{}`",
                    "üres class / id ebben: `{}`",
                    head
                )));
            }
            Mode::Class => classes.push(piece),
            Mode::Id => id = Some(piece),
        }
        Ok(())
    };
    for c in head.chars() {
        match c {
            '.' | '#' => {
                flush(mode, &mut buffer)?;
                mode = if c == '.' { Mode::Class } else { Mode::Id };
            }
            c if c.is_whitespace() => {
                return Err(GenError::new(crate::tf!(
                    "an element head has no spaces: `{}`",
                    "az elem-fejben nincs szóköz: `{}`",
                    head
                )));
            }
            c => buffer.push(c),
        }
    }
    flush(mode, &mut buffer)?;
    if tag.is_empty() {
        tag = "div".to_owned();
    }
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
    {
        return Err(GenError::new(crate::tf!(
            "bad tag name: `{}`",
            "hibás tag-név: `{}`",
            tag
        )));
    }
    Ok((tag, id, classes))
}

/// Az attribútum-lista szóközzel tagolt darabjai (idézőjelek / zárójelek között nem vág).
fn split_attrs(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let (mut depth, mut quote) = (0i32, None::<char>);
    for c in text.chars() {
        match quote {
            Some(q) => {
                current.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    current.push(c);
                }
                '(' | '[' | '{' => {
                    depth += 1;
                    current.push(c);
                }
                ')' | ']' | '}' => {
                    depth -= 1;
                    current.push(c);
                }
                c if c.is_whitespace() && depth <= 0 => {
                    if !current.is_empty() {
                        parts.push(std::mem::take(&mut current));
                    }
                }
                c => current.push(c),
            },
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn unquote(value: &str) -> String {
    for q in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(q) && value.ends_with(q) {
            return value[1..value.len() - 1].to_owned();
        }
    }
    value.to_owned()
}

fn parse_attrs(text: &str) -> GenResult<Vec<Attr>> {
    split_attrs(text)
        .into_iter()
        .map(|token| match token.split_once('=') {
            Some(("", _)) => Err(GenError::new(crate::tf!(
                "attribute without a name: `{}`",
                "név nélküli attribútum: `{}`",
                token
            ))),
            Some((name, value)) => Ok(Attr {
                name: name.to_owned(),
                value: Some(unquote(value)),
            }),
            None => Ok(Attr {
                name: token,
                value: None,
            }),
        })
        .collect()
}

fn rule(node: &Expanded) -> GenResult<Rule> {
    no_children(node)?;
    let mut selector = node.head.trim().to_owned();
    if let Some(list) = &node.list {
        selector = format!("{selector}[{list}]");
    }
    let Some(body) = node.raw.clone() else {
        return Err(GenError::new(crate::tf!(
            "the rule `{}` needs its declarations: `{}{{padding:1rem}}`",
            "a(z) `{}` szabályhoz kellenek a deklarációk: `{}{{padding:1rem}}`",
            selector,
            selector
        )));
    };
    Ok(Rule { selector, body })
}
