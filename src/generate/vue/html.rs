//! A HTML-generátor modellje, értelmezője és emitterje: a közös Emmet-fából (`parser::parse_tree`) `HtmlProgram`
//! (teljes dokumentum `html:Cím` fejjel, vagy csupasz elemek), abból behúzott HTML-sorok (2 szóköz, void elem
//! `<br>` alakban, SVG-ben önzáró). A nyelvtan a `vue/mod.rs` HTML-szakaszában.

use super::model::Attr;
use super::parse::{emmet_head, located, number_text, parse_attrs, rule, unquote};
use super::style;
use super::template::is_void;
use crate::generate::error::{GenError, GenResult};
use crate::generate::parser::{Expanded, parse_tree};

const IND: &str = "  ";
/// Ennél hosszabb nyitó taget az attribútumok soronkénti törésével írunk.
const WRAP: usize = 100;

/// A `link:rel=href` rövidítés ismert `rel` értékei (különben a teljes szöveg a stylesheet `href`-je).
const LINK_RELS: [&str; 11] = [
    "icon",
    "preload",
    "prefetch",
    "preconnect",
    "dns-prefetch",
    "canonical",
    "manifest",
    "apple-touch-icon",
    "alternate",
    "stylesheet",
    "modulepreload",
];

/// A `meta:og:title=X` típusú nevek, amelyek `property=` attribútumot kapnak (nem `name=`).
const PROPERTY_PREFIXES: [&str; 6] = ["og:", "article:", "fb:", "book:", "profile:", "music:"];

/// Egy elem vagy megjegyzés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HNode {
    Elem(HElement),
    Comment(String),
}

/// HTML-elem. `block`: a `style` / `script` nyers tartalma (sorok, szó szerint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HElement {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attrs: Vec<Attr>,
    pub text: Option<String>,
    pub block: Option<Vec<String>>,
    pub children: Vec<HNode>,
}

impl HElement {
    pub fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_owned(),
            id: None,
            classes: Vec::new(),
            attrs: Vec::new(),
            text: None,
            block: None,
            children: Vec::new(),
        }
    }

    fn with_attrs(tag: &str, attrs: &[(&str, Option<&str>)]) -> Self {
        let mut e = Self::new(tag);
        e.attrs = attrs
            .iter()
            .map(|(n, v)| Attr {
                name: (*n).to_owned(),
                value: v.map(str::to_owned),
            })
            .collect();
        e
    }
}

/// Teljes dokumentum: `html:Cím[lang=hu]>(head>…)+(body>…)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlDoc {
    pub title: Option<String>,
    /// A `<html>` elem (attribútumok; a `lang` mindig van, elöl).
    pub root: HElement,
    pub head: Vec<HNode>,
    pub body: HElement,
}

impl HtmlDoc {
    /// Csupasz elemekből dokumentum (`--write x.html` fej nélkül): a gyerekek a `<body>`-ba kerülnek.
    pub fn wrap(nodes: Vec<HNode>) -> Self {
        let mut root = HElement::new("html");
        root.attrs.push(Attr {
            name: "lang".to_owned(),
            value: Some("en".to_owned()),
        });
        let mut body = HElement::new("body");
        body.children = nodes;
        Self {
            title: None,
            root,
            head: Vec::new(),
            body,
        }
    }
}

/// Egy kifejezés eredménye.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlProgram {
    Document(Box<HtmlDoc>),
    Fragment(Vec<HNode>),
}

fn is_doc_head(head: &str) -> bool {
    head == "html"
        || head.starts_with("html:")
        || head.starts_with("html.")
        || head.starts_with("html#")
}

/// A kifejezés értelmezése.
pub fn parse(source: &str) -> GenResult<HtmlProgram> {
    let tree = parse_tree(source)?;
    if tree.len() == 1 && is_doc_head(&tree[0].head) {
        return document(&tree[0])
            .map(|doc| HtmlProgram::Document(Box::new(doc)))
            .map_err(|e| located(&tree[0], &e));
    }
    let mut nodes = Vec::new();
    for node in &tree {
        nodes.push(hnode(node).map_err(|e| located(node, &e))?);
    }
    Ok(HtmlProgram::Fragment(nodes))
}

fn children<'a>(nodes: impl IntoIterator<Item = &'a Expanded>) -> GenResult<Vec<HNode>> {
    nodes
        .into_iter()
        .map(|c| hnode(c).map_err(|e| located(c, &e)))
        .collect()
}

/// A fej tag-része (`body.home#x` → `body`; `meta:x=y` → `meta`).
fn head_tag(head: &str) -> &str {
    head.split(['.', '#', ':', '[', '{']).next().unwrap_or("")
}

fn document(node: &Expanded) -> GenResult<HtmlDoc> {
    let mut root = HElement::new("html");
    let mut title = None;
    if let Some(t) = node.head.strip_prefix("html:") {
        let t = unquote(t.trim());
        title = (!t.is_empty()).then_some(t);
    } else {
        let (_, id, classes) = emmet_head(&node.head)?;
        root.id = id;
        root.classes = classes;
    }
    if let Some(raw) = &node.raw
        && !raw.trim().is_empty()
    {
        title = Some(raw.trim().to_owned());
    }
    if let Some(list) = &node.list {
        root.attrs = parse_attrs(list)?;
    }
    let lang = root
        .attrs
        .iter()
        .position(|a| a.name == "lang")
        .map_or_else(
            || "en".to_owned(),
            |p| root.attrs.remove(p).value.unwrap_or_default(),
        );
    root.attrs.insert(
        0,
        Attr {
            name: "lang".to_owned(),
            value: Some(lang),
        },
    );
    let (mut head, mut body, mut loose): (Option<Vec<HNode>>, Option<HElement>, Vec<&Expanded>) =
        (None, None, Vec::new());
    for child in &node.children {
        match head_tag(&child.head) {
            "head" if head.is_none() => {
                head = Some(children(&child.children)?);
            }
            "body" if body.is_none() => {
                body = Some(element(child).map_err(|e| located(child, &e))?);
            }
            _ => loose.push(child),
        }
    }
    if !loose.is_empty() {
        if body.is_some() {
            return Err(GenError::new(
                crate::t!(
                    "with a `body>…` section every element belongs inside it",
                    "`body>…` szakasz mellett minden elemnek abban kell lennie"
                )
                .to_owned(),
            ));
        }
        let mut b = HElement::new("body");
        b.children = children(loose)?;
        body = Some(b);
    }
    Ok(HtmlDoc {
        title,
        root,
        head: head.unwrap_or_default(),
        body: body.unwrap_or_else(|| HElement::new("body")),
    })
}

fn hnode(node: &Expanded) -> GenResult<HNode> {
    if node.head == "comment" {
        if !node.children.is_empty() || node.list.is_some() {
            return Err(GenError::new(
                crate::t!(
                    "`comment{ text }` takes only its text",
                    "a `comment{ szöveg }` csak szöveget kap"
                )
                .to_owned(),
            ));
        }
        let text = number_text(node.raw.as_deref().unwrap_or(""), node.counter);
        if text.contains("-->") {
            return Err(GenError::new(
                crate::t!(
                    "a comment cannot contain `-->`",
                    "a megjegyzésben nem lehet `-->`"
                )
                .to_owned(),
            ));
        }
        return Ok(HNode::Comment(text));
    }
    if let Some(el) = shortcut(node)? {
        return Ok(HNode::Elem(el));
    }
    element(node).map(HNode::Elem)
}

/// `meta:` / `link:` / `script:` / `base:` rövidítések (a fejben `:` van).
fn shortcut(node: &Expanded) -> GenResult<Option<HElement>> {
    let Some((kind, rest)) = node.head.split_once(':') else {
        return Ok(None);
    };
    if !matches!(kind, "meta" | "link" | "script" | "base") {
        return Ok(None);
    }
    let rest = rest.trim();
    let missing = || {
        GenError::new(crate::tf!(
            "`{}:` needs a value (e.g. `meta:description=Text`, `link:style.css`, `script:app.js`, `base:/app/`)",
            "a(z) `{}:` után érték kell (pl. `meta:description=Text`, `link:style.css`, `script:app.js`, `base:/app/`)",
            kind
        ))
    };
    let mut el = match kind {
        "meta" => {
            let (name, value) = match rest.split_once('=') {
                Some((n, v)) => (n.trim().to_owned(), unquote(v.trim())),
                None => (rest.to_owned(), node.raw.clone().unwrap_or_default()),
            };
            if name.is_empty() || value.is_empty() {
                return Err(missing());
            }
            if name == "charset" {
                HElement::with_attrs("meta", &[("charset", Some(value.as_str()))])
            } else if let Some(equiv) = name.strip_prefix("http-equiv:") {
                HElement::with_attrs(
                    "meta",
                    &[
                        ("http-equiv", Some(equiv)),
                        ("content", Some(value.as_str())),
                    ],
                )
            } else {
                let key = if PROPERTY_PREFIXES.iter().any(|p| name.starts_with(p)) {
                    "property"
                } else {
                    "name"
                };
                HElement::with_attrs(
                    "meta",
                    &[
                        (key, Some(name.as_str())),
                        ("content", Some(value.as_str())),
                    ],
                )
            }
        }
        "link" => {
            let (rel, href) = match rest.split_once('=') {
                Some((l, r)) if LINK_RELS.contains(&l.trim()) => {
                    (l.trim().to_owned(), unquote(r.trim()))
                }
                _ => ("stylesheet".to_owned(), unquote(rest)),
            };
            if href.is_empty() {
                return Err(missing());
            }
            HElement::with_attrs(
                "link",
                &[("rel", Some(rel.as_str())), ("href", Some(href.as_str()))],
            )
        }
        "script" => {
            if rest.is_empty() {
                return Err(missing());
            }
            match rest.split_once('=') {
                Some(("module", src)) => {
                    let src = unquote(src.trim());
                    HElement::with_attrs(
                        "script",
                        &[("type", Some("module")), ("src", Some(src.as_str()))],
                    )
                }
                Some(("async", src)) => {
                    let src = unquote(src.trim());
                    HElement::with_attrs("script", &[("src", Some(src.as_str())), ("async", None)])
                }
                _ => {
                    let src = unquote(rest);
                    HElement::with_attrs("script", &[("src", Some(src.as_str())), ("defer", None)])
                }
            }
        }
        _ => {
            if rest.is_empty() {
                return Err(missing());
            }
            let href = unquote(rest);
            HElement::with_attrs("base", &[("href", Some(href.as_str()))])
        }
    };
    if let Some(list) = &node.list {
        el.attrs.extend(parse_attrs(list)?);
    }
    Ok(Some(el))
}

fn element(node: &Expanded) -> GenResult<HElement> {
    let (tag, id, classes) = emmet_head(&node.head)?;
    let attrs = match &node.list {
        Some(list) => parse_attrs(list)?,
        None => Vec::new(),
    };
    let mut el = HElement::new(&tag);
    el.id = id;
    el.classes = classes;
    el.attrs = attrs;
    match tag.as_str() {
        "style" | "script" if node.raw.is_some() => {
            if !node.children.is_empty() {
                return Err(GenError::new(crate::tf!(
                    "`{}{{ … }}` takes its code in the braces, not children",
                    "a `{}{{ … }}` a kódját a kapcsos zárójelben kapja, nem gyerekként",
                    tag
                )));
            }
            let raw = node.raw.as_deref().unwrap_or("");
            el.block = Some(raw.lines().map(str::to_owned).collect());
        }
        "style" if !node.children.is_empty() => {
            let rules = node
                .children
                .iter()
                .map(|c| rule(c).map_err(|e| located(c, &e)))
                .collect::<GenResult<Vec<_>>>()?;
            el.block = Some(style::lines(&rules));
        }
        _ => {
            el.children = children(&node.children)?;
            el.text = node
                .raw
                .as_deref()
                .map(|r| number_text(r, node.counter))
                .filter(|t| !t.is_empty());
        }
    }
    if is_void(&tag) && (!el.children.is_empty() || el.text.is_some()) {
        return Err(GenError::new(crate::tf!(
            "`<{}>` is a void element: no children or text",
            "a `<{}>` void elem: nem lehet gyereke vagy szövege",
            tag
        )));
    }
    Ok(el)
}

// ---- emitter ----------------------------------------------------------------------------------------------

fn attr_text(name: &str, value: Option<&str>) -> String {
    match value {
        None => name.to_owned(),
        Some(v) if v.contains('"') && !v.contains('\'') => format!("{name}='{v}'"),
        Some(v) => format!("{name}=\"{}\"", v.replace('"', "&quot;")),
    }
}

/// Az elem attribútumai kiírási sorrendben: `class`, `id`, a többi.
fn attributes(e: &HElement) -> Vec<String> {
    let mut classes: Vec<String> = e.classes.clone();
    let mut rest = Vec::new();
    for a in &e.attrs {
        match (&a.name[..], a.value.as_ref()) {
            ("class", Some(v)) => classes.extend(v.split_whitespace().map(str::to_owned)),
            _ => rest.push(attr_text(&a.name, a.value.as_deref())),
        }
    }
    let mut out = Vec::new();
    if !classes.is_empty() {
        out.push(attr_text("class", Some(&classes.join(" "))));
    }
    if let Some(id) = &e.id {
        out.push(attr_text("id", Some(id)));
    }
    out.extend(rest);
    out
}

/// A fa sorai (a legfelső szinten nincs behúzás).
pub fn lines(nodes: &[HNode]) -> Vec<String> {
    let mut out = Vec::new();
    for n in nodes {
        node(n, 0, false, &mut out);
    }
    out
}

fn node(n: &HNode, depth: usize, in_svg: bool, out: &mut Vec<String>) {
    match n {
        HNode::Elem(e) => element_lines(e, depth, in_svg, out),
        HNode::Comment(text) => comment(text, depth, out),
    }
}

fn comment(text: &str, depth: usize, out: &mut Vec<String>) {
    let pad = IND.repeat(depth);
    if text.contains('\n') {
        out.push(format!("{pad}<!--"));
        for l in text.lines() {
            if l.trim().is_empty() {
                out.push(String::new());
            } else {
                out.push(format!("{pad}{IND}{}", l.trim_end()));
            }
        }
        out.push(format!("{pad}-->"));
    } else {
        out.push(format!("{pad}<!-- {} -->", text.trim()));
    }
}

fn element_lines(e: &HElement, depth: usize, in_svg: bool, out: &mut Vec<String>) {
    let pad = IND.repeat(depth);
    let attrs = attributes(e);
    let bare = e.children.is_empty() && e.text.is_none() && e.block.is_none();
    let void = is_void(&e.tag);
    let self_closing = bare && in_svg && !void;
    let end_tok = if self_closing { "/>" } else { ">" };
    let one_line = format!(
        "{pad}<{}{}{}{end_tok}",
        e.tag,
        attrs.iter().map(|a| format!(" {a}")).collect::<String>(),
        if self_closing { " " } else { "" }
    );
    let wrapped = one_line.chars().count() > WRAP && attrs.len() > 1;
    if wrapped {
        out.push(format!("{pad}<{}", e.tag));
        out.extend(attrs.iter().map(|a| format!("{pad}{IND}{a}")));
        out.push(format!("{pad}{end_tok}"));
    } else {
        out.push(one_line);
    }
    if self_closing || (void && bare) {
        return;
    }
    let close = format!("</{}>", e.tag);
    if let Some(block) = &e.block {
        for l in block {
            if l.trim().is_empty() {
                out.push(String::new());
            } else {
                out.push(format!("{pad}{IND}{l}"));
            }
        }
        out.push(format!("{pad}{close}"));
        return;
    }
    if bare {
        if wrapped {
            out.push(format!("{pad}{close}"));
        } else if let Some(last) = out.last_mut() {
            last.push_str(&close);
        }
        return;
    }
    if e.children.is_empty() && !wrapped {
        let text = e.text.as_deref().unwrap_or("");
        let preformatted = matches!(e.tag.as_str(), "pre" | "textarea");
        if !text.contains('\n') {
            if let Some(last) = out.last_mut() {
                last.push_str(text);
                last.push_str(&close);
            }
            return;
        }
        if preformatted {
            let mut it = text.lines();
            if let (Some(first), Some(last)) = (it.next(), out.last_mut()) {
                last.push_str(first);
            }
            out.extend(it.map(str::to_owned));
            if let Some(last) = out.last_mut() {
                last.push_str(&close);
            }
            return;
        }
    }
    if let Some(text) = &e.text {
        for line in text.lines() {
            out.push(format!("{pad}{IND}{}", line.trim()));
        }
    }
    let nested = in_svg || e.tag == "svg" || e.tag == "math";
    for child in &e.children {
        node(child, depth + 1, nested, out);
    }
    out.push(format!("{pad}{close}"));
}

/// A teljes dokumentum sorai; `fallback_title`: ha nincs cím (a fájlnév törzse).
pub fn document_lines(doc: &HtmlDoc, fallback_title: &str) -> Vec<String> {
    let mut head = HElement::new("head");
    head.children.push(HNode::Elem(HElement::with_attrs(
        "meta",
        &[("charset", Some("utf-8"))],
    )));
    head.children.push(HNode::Elem(HElement::with_attrs(
        "meta",
        &[
            ("name", Some("viewport")),
            ("content", Some("width=device-width, initial-scale=1")),
        ],
    )));
    let has_title = doc
        .head
        .iter()
        .any(|n| matches!(n, HNode::Elem(e) if e.tag == "title"));
    if !has_title {
        let mut title = HElement::new("title");
        title.text = Some(
            doc.title
                .clone()
                .unwrap_or_else(|| fallback_title.to_owned()),
        );
        head.children.push(HNode::Elem(title));
    }
    head.children.extend(doc.head.iter().cloned());
    let mut out = vec!["<!doctype html>".to_owned()];
    let attrs = attributes(&doc.root);
    out.push(format!(
        "<html{}>",
        attrs.iter().map(|a| format!(" {a}")).collect::<String>()
    ));
    element_lines(&head, 0, false, &mut out);
    element_lines(&doc.body, 0, false, &mut out);
    out.push("</html>".to_owned());
    out
}
