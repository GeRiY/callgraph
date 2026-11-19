//! A Vue-nyelvjárású kifejezés értelmezése: a közös fa (`parser::parse_tree`) elemeiből `Program`
//! (script-tagok, template-elemek, stílus-szabályok). A fej-szintaxis a modul-dokumentációban (`vue/mod.rs`).

use super::model::{
    Attr, ClassDecl, ClassKind, ClassMember, CtorParam, Element, Emit, Export, FormField, Import,
    Kind, Member, Program, Prop, Rule, Script, Style, VParam, Vis,
};
use super::types::{parse_fields, parse_type};
use crate::generate::ast::TypeRef;
use crate::generate::error::{GenError, GenResult};
use crate::generate::head::{is_ident, matching, split_once_top, split_top};
use crate::generate::parser::{Expanded, parse_tree};

pub(super) fn located(node: &Expanded, e: &GenError) -> GenError {
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
        script.guess.append(&mut loose.guess);
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
    } else if let Some(name) = head.strip_prefix("react:") {
        program.kind = Kind::React;
        program.name = Some(ident(name)?);
        for child in &node.children {
            if matches!(child.head.as_str(), "jsx" | "template") {
                section(child, program).map_err(|e| located(child, &e))?;
            } else {
                let script = program.script.get_or_insert_with(Script::default);
                script_item(child, script).map_err(|e| located(child, &e))?;
            }
        }
    } else if let Some(name) = head.strip_prefix("hook:") {
        program.kind = Kind::Hook;
        program.name = Some(ident(name)?);
        let script = program.script.get_or_insert_with(Script::default);
        for child in &node.children {
            script_item(child, script).map_err(|e| located(child, &e))?;
        }
    } else if let Some(name) = head.strip_prefix("mod:") {
        program.kind = Kind::Module;
        program.name = Some(name.trim().to_owned());
        let script = program.script.get_or_insert_with(Script::default);
        for child in &node.children {
            script_item(child, script).map_err(|e| located(child, &e))?;
        }
    } else if let Some(name) = head.strip_prefix("comp:") {
        program.kind = Kind::Composable;
        program.name = Some(ident(name)?);
        let script = program.script.get_or_insert_with(Script::default);
        for child in &node.children {
            script_item(child, script).map_err(|e| located(child, &e))?;
        }
    } else if matches!(head, "script" | "template" | "jsx") || is_style_head(head) {
        if head == "jsx" {
            program.kind = Kind::React;
        }
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
        "template" | "jsx" => {
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
    if let Some(rest) = node.head.strip_prefix("use:") {
        no_children(node)?;
        let (imports, guess) = parse_uses(rest)?;
        script.imports.extend(imports);
        script.guess.extend(guess);
        return Ok(());
    }
    script.members.push(member_node(node)?);
    Ok(())
}

/// A fej az export-előtag nélkül és az export módja: `-fn:x` nem exportált, `!fn:x` `export default`.
fn split_export(head: &str) -> (Export, &str) {
    if let Some(rest) = head.strip_prefix('-') {
        (Export::None, rest)
    } else if let Some(rest) = head.strip_prefix('!') {
        (Export::Default, rest)
    } else {
        (Export::Named, head)
    }
}

/// Egy script-tag a gyerekeivel együtt: az export-előtag (`-` / `!`) és az osztály (`class:Név>…`) itt dől el;
/// minden más tag a `member`-é.
fn member_node(node: &Expanded) -> GenResult<Member> {
    let (export, bare) = split_export(&node.head);
    let inner = if bare.starts_with("class:") || bare.starts_with("class~:") {
        Member::Class(Box::new(class_decl(bare, node)?))
    } else {
        no_children(node)?;
        let plain = Expanded {
            head: bare.to_owned(),
            ..node.clone()
        };
        member(&plain)?
    };
    match export {
        Export::Named => Ok(inner),
        Export::None => Ok(Member::Exported {
            export,
            member: Box::new(inner),
        }),
        Export::Default => {
            if matches!(
                inner,
                Member::Func { .. } | Member::Const { .. } | Member::Class(_) | Member::Type { .. }
            ) {
                Ok(Member::Exported {
                    export,
                    member: Box::new(inner),
                })
            } else {
                Err(GenError::new(
                    crate::t!(
                        "`!` (export default) takes fn: / async: / const: / class: / type:Name{…}",
                        "a `!` (export default) fn: / async: / const: / class: / type:Név{…} előtt állhat"
                    )
                    .to_owned(),
                ))
            }
        }
    }
}

fn unclosed(open: char) -> GenError {
    GenError::new(crate::tf!("unclosed `{}`", "lezáratlan `{}`", open))
}

/// `class:Név<Ős>[A,B]` / `class~:Név` (absztrakt) és a gyerekei (osztálytagok).
fn class_decl(bare: &str, node: &Expanded) -> GenResult<ClassDecl> {
    let (is_abstract, rest) = match bare.strip_prefix("class~:") {
        Some(r) => (true, r),
        None => (false, bare.strip_prefix("class:").unwrap_or(bare)),
    };
    if node.raw.is_some() {
        return Err(GenError::new(
            crate::t!(
                "a class has no `{…}` body: its members follow after `>`",
                "az osztálynak nincs `{…}` törzse: a tagjai a `>` után jönnek"
            )
            .to_owned(),
        ));
    }
    let name_end = rest.find(['<', '[']).unwrap_or(rest.len());
    let name = ident(&rest[..name_end])?;
    let mut tail = rest[name_end..].trim();
    let mut extends = None;
    let mut implements: Vec<String> = Vec::new();
    if tail.starts_with('<') {
        let end = matching(tail, '<', '>').ok_or_else(|| unclosed('<'))?;
        extends = Some(tail[1..end].trim().to_owned()).filter(|e| !e.is_empty());
        tail = tail[end + 1..].trim();
    }
    if tail.starts_with('[') {
        let end = matching(tail, '[', ']').ok_or_else(|| unclosed('['))?;
        implements.extend(type_names(&tail[1..end]));
        tail = tail[end + 1..].trim();
    }
    if !tail.is_empty() {
        return Err(GenError::new(crate::tf!(
            "unexpected `{}` in the class head (e.g. `class:Pay<Base>[Chargeable]`)",
            "váratlan `{}` az osztály-fejben (pl. `class:Pay<Base>[Chargeable]`)",
            tail
        )));
    }
    if let Some(list) = &node.list {
        implements.extend(type_names(list));
    }
    let members = node
        .children
        .iter()
        .map(|c| class_member(c).map_err(|e| located(c, &e)))
        .collect::<GenResult<Vec<_>>>()?;
    Ok(ClassDecl {
        name,
        is_abstract,
        extends,
        implements,
        members,
    })
}

/// Vesszővel tagolt típusnevek (üres darabok nélkül).
fn type_names(text: &str) -> Vec<String> {
    split_top(text, ',')
        .iter()
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Egy osztálytag feje: `+` / `-` / `#` láthatóság, majd `::` static, `~` abstract, `%` readonly, végül
/// `ctor(…)`, `$mező:típus=érték` vagy `név(paraméterek):Típus{törzs}` (`fn:` / `async:` előtaggal is).
fn class_member(node: &Expanded) -> GenResult<ClassMember> {
    no_children(node)?;
    if node.list.is_some() {
        return Err(GenError::new(
            crate::t!(
                "`[…]` belongs to a class head (`class:Name[Iface]`)",
                "a `[…]` az osztály-fejhez tartozik (`class:Név[Interfész]`)"
            )
            .to_owned(),
        ));
    }
    let raw = node.raw.as_deref();
    let mut s = node.head.as_str();
    let mut vis = Vis::Public;
    if let Some(rest) = s.strip_prefix('+') {
        s = rest;
    } else if let Some(rest) = s.strip_prefix('-') {
        vis = Vis::Private;
        s = rest;
    } else if let Some(rest) = s.strip_prefix('#') {
        vis = Vis::Protected;
        s = rest;
    }
    let (mut is_static, mut is_abstract, mut readonly) = (false, false, false);
    loop {
        if let Some(rest) = s.strip_prefix("::") {
            is_static = true;
            s = rest;
        } else if let Some(rest) = s.strip_prefix('~') {
            is_abstract = true;
            s = rest;
        } else if let Some(rest) = s.strip_prefix('%') {
            readonly = true;
            s = rest;
        } else {
            break;
        }
    }
    let kind = if let Some(rest) = s.strip_prefix('$') {
        let (name, ty, init) = decl(rest, raw)?;
        ClassKind::Field { name, ty, init }
    } else if s == "ctor" || call_args(s, "ctor").is_some() {
        ClassKind::Ctor {
            params: ctor_params(call_args(s, "ctor").unwrap_or(""))?,
            body: raw.map(str::to_owned),
        }
    } else {
        let (is_async, rest) = if let Some(r) = s.strip_prefix("async:") {
            (true, r)
        } else {
            (false, s.strip_prefix("fn:").unwrap_or(s))
        };
        let Member::Func {
            name,
            params,
            ret,
            body,
            is_async,
        } = func(rest, raw, is_async)?
        else {
            return Err(GenError::new(crate::tf!(
                "bad class member `{}`",
                "hibás osztálytag: `{}`",
                s
            )));
        };
        ClassKind::Method {
            name,
            params,
            ret,
            body,
            is_async,
        }
    };
    Ok(ClassMember {
        vis,
        is_static,
        is_abstract,
        readonly,
        kind,
    })
}

/// A konstruktor paraméterei: a `+` / `-` / `#` (és `%`) előtagos paraméter property lesz; a `-` / `#` readonly is.
fn ctor_params(text: &str) -> GenResult<Vec<CtorParam>> {
    let mut out = Vec::new();
    for piece in split_top(text, ',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let (vis, rest) = if let Some(r) = piece.strip_prefix('+') {
            (Some(Vis::Public), r)
        } else if let Some(r) = piece.strip_prefix('-') {
            (Some(Vis::Private), r)
        } else if let Some(r) = piece.strip_prefix('#') {
            (Some(Vis::Protected), r)
        } else {
            (None, piece)
        };
        let (forced, rest) = match rest.strip_prefix('%') {
            Some(r) => (true, r),
            None => (false, rest),
        };
        let mut parsed = parse_params(rest)?;
        let Some(param) = parsed.pop().filter(|_| parsed.is_empty()) else {
            return Err(GenError::new(crate::tf!(
                "bad constructor parameter: `{}`",
                "hibás konstruktor-paraméter: `{}`",
                piece
            )));
        };
        let readonly = vis.is_some() && (forced || vis != Some(Vis::Public));
        out.push(CtorParam {
            param,
            vis,
            readonly,
        });
    }
    Ok(out)
}

/// Osztálytagok kifejezése az `--into x.ts::Osztály` / `--replace x.ts::Osztály::tag` célra (csupasz tagok és `use:`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClassParts {
    pub members: Vec<ClassMember>,
    pub imports: Vec<Import>,
    pub guess: Vec<String>,
}

pub fn parse_class_members(source: &str) -> GenResult<ClassParts> {
    let tree = parse_tree(source)?;
    let mut parts = ClassParts::default();
    for node in &tree {
        if let Some(rest) = node.head.strip_prefix("use:") {
            let (imports, guess) = parse_uses(rest).map_err(|e| located(node, &e))?;
            parts.imports.extend(imports);
            parts.guess.extend(guess);
        } else {
            parts
                .members
                .push(class_member(node).map_err(|e| located(node, &e))?);
        }
    }
    if parts.members.is_empty() {
        return Err(GenError::new(
            crate::t!(
                "nothing to insert: the expression has no class member (e.g. `+run(n:int):void{ … }`)",
                "nincs mit beszúrni: a kifejezésben nincs osztálytag (pl. `+run(n:int):void{ … }`)"
            )
            .to_owned(),
        ));
    }
    Ok(parts)
}

/// Alias-típus (`type:Név=A|B`): a `|` darabjai lehetnek idézőjeles / számliterálok (`'a'|'b'`) is.
fn alias_type(text: &str) -> GenResult<TypeRef> {
    let one = |p: &str| -> GenResult<TypeRef> {
        let p = p.trim();
        let literal =
            p.starts_with(['\'', '"', '`']) || p.starts_with(|c: char| c.is_ascii_digit());
        if literal {
            Ok(TypeRef::Named(p.to_owned()))
        } else {
            parse_type(p)
        }
    };
    let parts = split_top(text, '|');
    if parts.len() > 1 {
        let members = parts
            .iter()
            .map(|p| one(p))
            .collect::<GenResult<Vec<_>>>()?;
        Ok(TypeRef::Union(members))
    } else {
        one(text)
    }
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
    if head == "raw" {
        let body = raw.map(str::trim).filter(|b| !b.is_empty());
        return body.map(|b| Member::Raw(b.to_owned())).ok_or_else(|| {
            GenError::new(
                crate::t!(
                    "`raw{…}` needs its statements: `raw{ const { a } = useX() }`",
                    "a `raw{…}`-hoz kellenek az utasítások: `raw{ const { a } = useX() }`"
                )
                .to_owned(),
            )
        });
    }
    if let Some(rest) = head.strip_prefix("type:") {
        if let Some((name, rhs)) = split_once_top(rest, '=') {
            let text = if rhs.trim().is_empty() {
                raw.map(|r| format!("{{{r}}}")).unwrap_or_default()
            } else {
                rhs.trim().to_owned()
            };
            if text.is_empty() {
                return Err(GenError::new(
                    crate::t!(
                        "`type:Name=…` needs its type: `type:Id=str|int`",
                        "a `type:Név=…`-hez kell a típus: `type:Id=str|int`"
                    )
                    .to_owned(),
                ));
            }
            return Ok(Member::Alias {
                name: ident(name)?,
                ty: alias_type(&text)?,
            });
        }
        let name = rest;
        let Some(body) = raw else {
            return Err(GenError::new(
                crate::t!(
                    "`type:Name{…}` needs its fields: `type:Flag{flag:str,on?:bool}`",
                    "a `type:Név{…}`-hez kellenek a mezők: `type:Flag{flag:str,on?:bool}`"
                )
                .to_owned(),
            ));
        };
        return Ok(Member::Type {
            name: ident(name)?,
            fields: parse_fields(body)?,
        });
    }
    for (prefix, mutable) in [("const:", false), ("let:", true)] {
        if let Some(rest) = head.strip_prefix(prefix) {
            if rest.trim_start().starts_with('{') {
                return destructure(rest, raw, mutable);
            }
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
        "unknown script member `{}` (props(…) emits(…) ref:n:T=v computed:n:T{{…}} fn:n(p:T):R{{…}} form(…) const:n:T=v const:{{a,b}}=f() type:N{{a:str}} raw{{…}} on:mounted{{…}} use:path)",
        "ismeretlen script-tag: `{}` (props(…) emits(…) ref:n:T=v computed:n:T{{…}} fn:n(p:T):R{{…}} form(…) const:n:T=v const:{{a,b}}=f() type:N{{a:str}} raw{{…}} on:mounted{{…}} use:út)",
        head
    )))
}

/// `{ a, b: c, ...rest }=érték` → destructuring (a `const:` / `let:` után).
fn destructure(rest: &str, raw: Option<&str>, mutable: bool) -> GenResult<Member> {
    let rest = rest.trim();
    let close = matching(rest, '{', '}')
        .ok_or_else(|| GenError::new(crate::t!("unclosed `{`", "lezáratlan `{`").to_owned()))?;
    let mut names = Vec::new();
    let mut pieces = Vec::new();
    for piece in split_top(&rest[1..close], ',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let bound = piece.strip_prefix("...").unwrap_or(piece);
        let bound = split_once_top(bound, '=').map_or(bound, |(l, _)| l);
        let bound = split_once_top(bound, ':').map_or(bound, |(_, r)| r);
        names.push(ident(bound)?);
        pieces.push(piece.to_owned());
    }
    let missing = || {
        GenError::new(
            crate::t!(
                "the destructuring needs its source: `const:{ a, b }=useX()`",
                "a destructuring-hoz kell a forrás: `const:{ a, b }=useX()`"
            )
            .to_owned(),
        )
    };
    if names.is_empty() {
        return Err(missing());
    }
    let tail = rest[close + 1..].trim();
    let value = match tail.strip_prefix('=') {
        Some(v) => v.trim().to_owned(),
        None => raw.map(str::trim).unwrap_or_default().to_owned(),
    };
    if value.is_empty() {
        return Err(missing());
    }
    Ok(Member::Destructure {
        names,
        pattern: pieces.join(", "),
        value,
        mutable,
    })
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
        Some((name, ty)) => Ok((ident(name)?, Some(parse_type(ty)?))),
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
        Some(ty) => Some(parse_type(ty)?),
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
                Some((n, t)) => (n.trim(), Some(alias_type(t)?)),
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
                Some((n, t)) => (n.trim(), Some(alias_type(t)?)),
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

/// `{useX}@/út`, `{a,b as c}@/út` → nevesített import.
fn braced_import(item: &str) -> GenResult<Import> {
    let close = matching(item, '{', '}')
        .ok_or_else(|| GenError::new(crate::t!("unclosed `{`", "lezáratlan `{`").to_owned()))?;
    let source = item[close + 1..].trim();
    if source.is_empty() {
        return Err(GenError::new(
            crate::t!(
                "a named import needs its path: `use:{useX}@/composables/useX`",
                "a nevesített importhoz út kell: `use:{useX}@/composables/useX`"
            )
            .to_owned(),
        ));
    }
    let mut named = Vec::new();
    for piece in split_top(&item[1..close], ',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        named.push(match piece.split_once(" as ") {
            Some((n, a)) => format!("{} as {}", ident(n)?, ident(a)?),
            None => ident(piece)?,
        });
    }
    if named.is_empty() {
        return Err(GenError::new(
            crate::t!(
                "the braces of a named import are empty: `use:{useX}@/composables/useX`",
                "a nevesített import kapcsos zárójele üres: `use:{useX}@/composables/useX`"
            )
            .to_owned(),
        ));
    }
    Ok(Import {
        source: source.to_owned(),
        default: None,
        named,
    })
}

/// `use:` lista: `@/Components/Card`, `@/Components/Card as C`, `vue(ref,computed)`, `{useX}@/composables/useX`,
/// `{a,b as c}@/út`. A második érték a kiterjesztés / alias / kapcsos zárójel nélküli importok forrásai: hogy
/// nevesített-e, a generátor dönti el a célfájl alapján.
fn parse_uses(rest: &str) -> GenResult<(Vec<Import>, Vec<String>)> {
    let mut out = Vec::new();
    let mut guess = Vec::new();
    for item in split_top(rest, ',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if item.starts_with('{') {
            out.push(braced_import(item)?);
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
        if path.contains("::") {
            return Err(GenError::new(crate::tf!(
                "`::` is not part of an import path (`{}`): a named import is `use:{{useX}}@/composables/useX`",
                "a `::` nem része az import-útnak (`{}`): nevesített import: `use:{{useX}}@/composables/useX`",
                path
            )));
        }
        let file = path.rsplit('/').next().unwrap_or(path);
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        let guessable = alias.is_none();
        let default = match alias {
            Some(a) => a,
            None => ident(stem).map_err(|_| {
                GenError::new(crate::tf!(
                    "cannot name the import of `{}`: use `{{Name}}{}` (named) or `{} as Name` (default)",
                    "nem tudom elnevezni a(z) `{}` importját: használd így: `{{Név}}{}` (nevesített) vagy `{} as Név` (default)",
                    path,
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
        if guessable && !component && !file.contains('.') {
            guess.push(source.clone());
        }
        out.push(Import {
            source,
            default: Some(default),
            named: Vec::new(),
        });
    }
    Ok((out, guess))
}

/// A számláló (`$`) cseréje a szövegben: csak az önálló `$` (nem `$emit`, nem `a$b`).
pub(super) fn number_text(text: &str, counter: Option<u32>) -> String {
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
    // `span{{{ x }}}` / `p{{{ a }} / {{ b }}}`: a törzs már maga mustache-sal kezdődik, nem kell köré újabb pár
    if text.starts_with("{{") {
        return Some(text);
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
pub(super) fn emmet_head(head: &str) -> GenResult<(String, Option<String>, Vec<String>)> {
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

pub(super) fn unquote(value: &str) -> String {
    for q in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(q) && value.ends_with(q) {
            return value[1..value.len() - 1].to_owned();
        }
    }
    value.to_owned()
}

pub(super) fn parse_attrs(text: &str) -> GenResult<Vec<Attr>> {
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

pub(super) fn rule(node: &Expanded) -> GenResult<Rule> {
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
