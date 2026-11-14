//! Interpreter minta, 2. lépés: a tokenekből fa (`>` gyerek, `+` testvér, `^` fel, `( )` csoport, `*n` ismétlés),
//! a fából az ismétlések és a `$` számlálók kifejtése után a nyelvfüggetlen AST (`Program`). A fej-szintaxis a
//! `head` modulban él.

use super::ast::{Member, Program, TypeDecl, UseImport};
use super::error::{GenError, GenResult};
use super::head;
use super::lexer::{Lexer, Tok};

/// Az elem nyers adatai a fában.
#[derive(Debug, Clone)]
struct Elem {
    head: String,
    raw: Option<String>,
    list: Option<String>,
    col: usize,
}

#[derive(Debug, Clone)]
enum NodeKind {
    Elem(Elem),
    Group,
}

#[derive(Debug, Clone)]
struct Node {
    kind: NodeKind,
    repeat: Option<u32>,
    children: Vec<Node>,
}

/// Egy kifejtett elem.
enum Item {
    Ns(String),
    Use(Vec<UseImport>),
    Decl(TypeDecl),
    Member(Member),
}

struct Parser {
    lexer: Lexer,
    pending: Option<Tok>,
    /// Vue-nyelvjárás: a `style` alatti elemek szelektorok (szóközzel).
    style_aware: bool,
}

/// A kifejezés értelmezése: tokenizálás → fa → kifejtés → `Program`.
pub fn parse(source: &str) -> GenResult<Program> {
    if source.trim().is_empty() {
        return Err(GenError::new(crate::t!(
            "empty expression (e.g. `c!:Pay<Base>[Chargeable] > ctor(-Client $http) + +charge(Order $o):Receipt`)",
            "üres kifejezés (pl. `c!:Pay<Base>[Chargeable] > ctor(-Client $http) + +charge(Order $o):Receipt`)"
        )));
    }
    let mut parser = Parser {
        lexer: Lexer::new(source),
        pending: None,
        style_aware: false,
    };
    let (nodes, end) = parser.seq()?;
    if end != Tok::End {
        return Err(parser.lexer.error_at(
            parser.lexer.token_col(),
            crate::t!("unmatched `)`", "nincs párja a `)` jelnek"),
        ));
    }
    assemble(expand(&nodes, None)?)
}

fn level<'a>(roots: &'a mut Vec<Node>, path: &[usize]) -> &'a mut Vec<Node> {
    let mut current = roots;
    for &i in path {
        current = &mut current[i].children;
    }
    current
}

/// A `path` által kijelölt szülők között van-e `style` szakasz (`style`, `style!`, `style:scoped`, …).
fn under_style(roots: &[Node], path: &[usize]) -> bool {
    let mut current = roots;
    for &i in path {
        let Some(node) = current.get(i) else {
            return false;
        };
        if let NodeKind::Elem(e) = &node.kind {
            let style =
                e.head == "style" || e.head.starts_with("style!") || e.head.starts_with("style:");
            if style {
                return true;
            }
        }
        current = &node.children;
    }
    false
}

impl Parser {
    fn take_op(&mut self) -> GenResult<Tok> {
        match self.pending.take() {
            Some(tok) => Ok(tok),
            None => self.lexer.op(),
        }
    }

    /// Testvérek sorozata a `)`-ig vagy a végéig; a lezáró tokent visszaadja.
    fn seq(&mut self) -> GenResult<(Vec<Node>, Tok)> {
        let mut roots: Vec<Node> = Vec::new();
        let mut path: Vec<usize> = Vec::new();
        loop {
            if self.style_aware {
                let under_style = under_style(&roots, &path);
                self.lexer.set_selector(under_style);
            }
            let node = self.item()?;
            level(&mut roots, &path).push(node);
            let mut tok = self.take_op()?;
            loop {
                match tok {
                    Tok::End | Tok::Close => return Ok((roots, tok)),
                    Tok::Sibling | Tok::NewlineSibling => break,
                    Tok::Child => {
                        let siblings = level(&mut roots, &path);
                        let last = siblings.len() - 1;
                        if matches!(siblings[last].kind, NodeKind::Group) {
                            return Err(self.lexer.error_at(
                                self.lexer.token_col(),
                                crate::t!(
                                    "`>` after a group is not supported: put the children inside the group",
                                    "`>` csoport után nem támogatott: a gyerekeket tedd a csoportba"
                                ),
                            ));
                        }
                        path.push(last);
                        break;
                    }
                    Tok::Up => {
                        if path.pop().is_none() {
                            return Err(self.lexer.error_at(
                                self.lexer.token_col(),
                                crate::t!(
                                    "`^` at the top level (nothing to climb out of)",
                                    "`^` a legfelső szinten (nincs honnan feljebb lépni)"
                                ),
                            ));
                        }
                        // egymás utáni `^` további szinteket lép; az utána jövő `+` elhagyható
                        tok = self.lexer.after_up()?;
                    }
                    other => {
                        return Err(self.lexer.error_at(
                            self.lexer.token_col(),
                            &crate::tf!("unexpected {:?}", "váratlan elem: {:?}", other),
                        ));
                    }
                }
            }
        }
    }

    /// Egy elem (szó + `{…}` / `[…]` / `*n` toldalékok) vagy egy csoport.
    fn item(&mut self) -> GenResult<Node> {
        let first = self.lexer.item()?;
        let col = self.lexer.token_col();
        let (kind, children) = match first {
            Tok::Word(head) => (
                NodeKind::Elem(Elem {
                    head,
                    raw: None,
                    list: None,
                    col,
                }),
                Vec::new(),
            ),
            Tok::Open => {
                let (kids, end) = self.seq()?;
                if end != Tok::Close {
                    return Err(self
                        .lexer
                        .error_at(col, crate::t!("unclosed `(`", "lezáratlan `(`")));
                }
                (NodeKind::Group, kids)
            }
            _ => {
                return Err(self.lexer.error_at(
                    col,
                    crate::t!(
                        "expected an element after the operator",
                        "az operátor után elemet várok"
                    ),
                ));
            }
        };
        let mut node = Node {
            kind,
            repeat: None,
            children,
        };
        loop {
            let tok = self.lexer.op()?;
            match (tok, &mut node.kind) {
                (Tok::Repeat(n), _) => node.repeat = Some(n),
                (Tok::Raw(text), NodeKind::Elem(e)) if e.raw.is_none() => e.raw = Some(text),
                (Tok::List(text), NodeKind::Elem(e)) if e.list.is_none() => e.list = Some(text),
                (Tok::Raw(_) | Tok::List(_), _) => {
                    return Err(self.lexer.error_at(
                        self.lexer.token_col(),
                        crate::t!(
                            "one `{…}` body and one `[…]` list per element",
                            "elemenként egy `{…}` törzs és egy `[…]` lista lehet"
                        ),
                    ));
                }
                (other, _) => {
                    self.pending = Some(other);
                    break;
                }
            }
        }
        Ok(node)
    }
}

/// Az ismétlések és a számlálók kifejtése; a csoportok a szülő szintjére simulnak.
fn expand(nodes: &[Node], counter: Option<u32>) -> GenResult<Vec<Item>> {
    let mut out = Vec::new();
    for node in nodes {
        for i in 1..=node.repeat.unwrap_or(1) {
            let count = if node.repeat.is_some() {
                Some(i)
            } else {
                counter
            };
            match &node.kind {
                NodeKind::Group => out.extend(expand(&node.children, count)?),
                NodeKind::Elem(e) => out.push(build(e, &node.children, count)?),
            }
        }
    }
    Ok(out)
}

/// A `$` számláló a név végén (`Item$` → `Item1`, `$$` → két számjegyre nullával kitöltve); a zárójeleken
/// belül (paraméterek) és az elem elején álló `$` (property-jel) érintetlen.
fn apply_counter(head: &str, counter: Option<u32>) -> String {
    let n = counter.unwrap_or(1);
    let chars: Vec<char> = head.chars().collect();
    let mut out = String::new();
    let mut depth = 0i32;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        let after_name = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if c == '$' && depth == 0 && after_name {
            let mut width = 0usize;
            while chars.get(i + width) == Some(&'$') {
                width += 1;
            }
            out.push_str(&format!("{n:0width$}"));
            i += width;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn build(elem: &Elem, children: &[Node], counter: Option<u32>) -> GenResult<Item> {
    let text = apply_counter(&elem.head, counter);
    let located = |e: GenError| {
        GenError::new(crate::tf!(
            "{} (col {}: `{}`)",
            "{} ({}. oszlop: `{}`)",
            e.message(),
            elem.col,
            text
        ))
    };
    let no_children = |what: &str| {
        if children.is_empty() {
            Ok(())
        } else {
            Err(located(GenError::new(crate::tf!(
                "{} takes no children (`>`)",
                "{} nem kaphat gyerekeket (`>`)",
                what
            ))))
        }
    };
    if let Some(rest) = text.strip_prefix("ns:") {
        no_children("ns:")?;
        return head::parse_ns(rest).map(Item::Ns).map_err(located);
    }
    if let Some(rest) = text.strip_prefix("use:") {
        no_children("use:")?;
        return head::parse_uses(rest).map(Item::Use).map_err(located);
    }
    if head::is_decl_head(&text) {
        if elem.raw.is_some() {
            return Err(located(GenError::new(
                crate::t!(
                    "a type has no `{…}` body: members follow after `>`",
                    "a típusnak nincs `{…}` törzse: a tagok a `>` után jönnek"
                )
                .to_owned(),
            )));
        }
        let mut decl = head::parse_decl(&text, elem.list.as_deref()).map_err(located)?;
        for item in expand(children, counter)? {
            match item {
                Item::Member(m) => decl.members.push(m),
                _ => {
                    return Err(located(GenError::new(
                        crate::t!(
                            "only members (methods, properties, constants, ctor) can go under a type",
                            "típus alá csak tag (metódus, property, konstans, ctor) kerülhet"
                        )
                        .to_owned(),
                    )));
                }
            }
        }
        return Ok(Item::Decl(decl));
    }
    no_children(crate::t!("a member", "a tag"))?;
    if elem.list.is_some() {
        return Err(located(GenError::new(
            crate::t!(
                "`[…]` belongs to a type (`c:Name[Iface]`)",
                "a `[…]` típushoz tartozik (`c:Név[Interfész]`)"
            )
            .to_owned(),
        )));
    }
    head::parse_member(&text, elem.raw.as_deref())
        .map(Item::Member)
        .map_err(located)
}

fn assemble(items: Vec<Item>) -> GenResult<Program> {
    let mut program = Program::default();
    for item in items {
        match item {
            Item::Ns(ns) => program.namespace = Some(ns),
            Item::Use(uses) => program.uses.extend(uses),
            Item::Decl(decl) => program.decls.push(decl),
            Item::Member(member) => program.loose.push(member),
        }
    }
    if !program.decls.is_empty() && !program.loose.is_empty() {
        return Err(GenError::new(
            crate::t!(
                "bare members and types cannot be mixed at the top level: put the members under a type with `>`",
                "csupasz tagot és típust nem lehet a legfelső szinten keverni: a tagokat `>`-jellel tedd a típus alá"
            )
            .to_owned(),
        ));
    }
    if program.decls.is_empty() && program.loose.is_empty() {
        return Err(GenError::new(
            crate::t!(
                "nothing to generate: the expression has only `ns:` / `use:`",
                "nincs mit generálni: a kifejezésben csak `ns:` / `use:` van"
            )
            .to_owned(),
        ));
    }
    Ok(program)
}

/// Egy kifejtett elem a nyelvfüggetlen fában (a Vue-nyelvjárás kimenete): a fej a `$` számlálóval behelyettesítve,
/// a nyers törzs / lista még nyers; az értelmezés a célnyelv dolga (`generate::vue`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    pub head: String,
    pub raw: Option<String>,
    pub list: Option<String>,
    /// A fej kezdő oszlopa (1-alapú) a hibaüzenetekhez.
    pub col: usize,
    /// Az ismétlés sorszáma (`*3` → 1, 2, 3), ha az elem vagy egy őse ismétlődik.
    pub counter: Option<u32>,
    pub children: Vec<Expanded>,
}

/// Vue-nyelvjárású értelmezés: tokenizálás → fa → kifejtés, a PHP-s elem-értelmezés nélkül (az értelmezést a hívó
/// végzi az `Expanded` fán).
pub fn parse_tree(source: &str) -> GenResult<Vec<Expanded>> {
    if source.trim().is_empty() {
        return Err(GenError::new(crate::t!(
            "empty expression (e.g. `vue:Card > script > props(title:str) + template > div.card > h1{{ title }}`)",
            "üres kifejezés (pl. `vue:Card > script > props(title:str) + template > div.card > h1{{ title }}`)"
        )));
    }
    let mut parser = Parser {
        lexer: Lexer::vue(source),
        pending: None,
        style_aware: true,
    };
    let (nodes, end) = parser.seq()?;
    if end != Tok::End {
        return Err(parser.lexer.error_at(
            parser.lexer.token_col(),
            crate::t!("unmatched `)`", "nincs párja a `)` jelnek"),
        ));
    }
    expand_tree(&nodes, None)
}

/// A fej minden (zárójelen kívüli) `$`-sorozata a számláló értékére (`$$` kitöltve); csak ismétlésen belül.
fn apply_counter_any(head: &str, counter: Option<u32>) -> String {
    let Some(n) = counter else {
        return head.to_owned();
    };
    let chars: Vec<char> = head.chars().collect();
    let mut out = String::new();
    let (mut depth, mut i) = (0i32, 0usize);
    while i < chars.len() {
        match chars[i] {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if chars[i] == '$' && depth == 0 {
            let mut width = 0usize;
            while chars.get(i + width) == Some(&'$') {
                width += 1;
            }
            out.push_str(&format!("{n:0width$}"));
            i += width;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn expand_tree(nodes: &[Node], counter: Option<u32>) -> GenResult<Vec<Expanded>> {
    let mut out = Vec::new();
    for node in nodes {
        for i in 1..=node.repeat.unwrap_or(1) {
            let count = if node.repeat.is_some() {
                Some(i)
            } else {
                counter
            };
            match &node.kind {
                NodeKind::Group => out.extend(expand_tree(&node.children, count)?),
                NodeKind::Elem(e) => out.push(Expanded {
                    head: apply_counter_any(&e.head, count),
                    raw: e.raw.clone(),
                    list: e.list.clone(),
                    col: e.col,
                    counter: count,
                    children: expand_tree(&node.children, count)?,
                }),
            }
        }
    }
    Ok(out)
}
