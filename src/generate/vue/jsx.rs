//! The JSX emitter: the template `Element` tree (the same Emmet-like expression the Vue generator reads) as JSX lines.
//! The attribute spelling is translated: `class` -> `className`, `for` -> `htmlFor`, `:title=x` -> `title={x}`,
//! `@click=f` -> `onClick={f}`, `v-if="c"` -> `{c && (…)}`, `v-for="x in xs"` -> `{xs.map((x) => (…))}`; the text's
//! `{{ expr }}` becomes `{expr}`.

use super::model::{Attr, Element};
use super::template::{is_component, is_void};
use crate::generate::error::{GenError, GenResult};

/// One indent level (the caller re-indents to the project's unit).
const IND: &str = "  ";
/// A longer opening tag is written one attribute per line.
const WRAP: usize = 100;

/// An attribute as JSX text.
fn attr_text(name: &str, value: Option<&str>) -> String {
    let Some(v) = value else {
        return name.to_owned();
    };
    if v.starts_with('{') && v.ends_with('}') {
        return format!("{name}={v}");
    }
    if v.contains('"') {
        format!("{name}='{v}'")
    } else {
        format!("{name}=\"{v}\"")
    }
}

/// `@click` -> `onClick`, `@update:value` is not an event of a DOM element: only plain names.
fn event_name(raw: &str) -> GenResult<String> {
    if raw.contains(['.', ':']) {
        return Err(GenError::new(crate::tf!(
            "`@{}`: event modifiers are not available in JSX; call `e.preventDefault()` in the handler",
            "`@{}`: JSX-ben nincs eseménymódosító; a kezelőben hívd az `e.preventDefault()`-ot",
            raw
        )));
    }
    let mut chars = raw.chars();
    let first = chars.next().map(|c| c.to_ascii_uppercase());
    Ok(format!(
        "on{}{}",
        first.map(String::from).unwrap_or_default(),
        chars.as_str()
    ))
}

/// The translated attributes of an element: `(name, value)` pairs ready for `attr_text`, plus the `v-if` / `v-for`.
struct Translated {
    attrs: Vec<String>,
    cond: Option<String>,
    each: Option<String>,
}

fn translate(e: &Element) -> GenResult<Translated> {
    let mut classes: Vec<String> = e.classes.clone();
    let mut out = Translated {
        attrs: Vec::new(),
        cond: None,
        each: None,
    };
    let mut rest: Vec<String> = Vec::new();
    for Attr { name, value } in &e.attrs {
        let v = value.as_deref();
        match name.as_str() {
            "class" | "className" => {
                if let Some(v) = v {
                    classes.extend(v.split_whitespace().map(str::to_owned));
                }
            }
            "v-if" => out.cond = v.map(str::to_owned),
            "v-for" => out.each = v.map(str::to_owned),
            "v-model" => {
                return Err(GenError::new(
                    crate::t!(
                        "`v-model` has no JSX equivalent: write `:value=x @change=f` (a controlled input)",
                        "a `v-model`-nek nincs JSX-megfelelője: írd `:value=x @change=f` alakban (kontrollált input)"
                    )
                    .to_owned(),
                ));
            }
            "for" => rest.push(attr_text("htmlFor", v)),
            n if n.starts_with(':') => {
                let expr = v.unwrap_or("true");
                rest.push(format!("{}={{{expr}}}", &n[1..]));
            }
            n if n.starts_with('@') => {
                rest.push(format!("{}={{{}}}", event_name(&n[1..])?, v.unwrap_or("")));
            }
            n if n.starts_with('#') => {
                return Err(GenError::new(crate::tf!(
                    "`{}` is a Vue slot: pass children or a prop in JSX",
                    "a `{}` Vue-slot: JSX-ben gyerek vagy prop kell",
                    n
                )));
            }
            n => rest.push(attr_text(n, v)),
        }
    }
    if !classes.is_empty() {
        out.attrs
            .push(attr_text("className", Some(&classes.join(" "))));
    }
    if let Some(id) = &e.id {
        out.attrs.push(attr_text("id", Some(id)));
    }
    out.attrs.extend(rest);
    Ok(out)
}

/// The element text: the Vue mustache `{{ expr }}` becomes `{expr}`; other text is kept.
fn text_of(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let Some(close) = rest[open + 2..].find("}}") else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&format!("{{{}}}", rest[open + 2..open + 2 + close].trim()));
        rest = &rest[open + 2 + close + 2..];
    }
    out.push_str(rest);
    out
}

/// The JSX lines of a tree (no indent at the top level); several roots are wrapped in a fragment.
pub fn lines(elements: &[Element]) -> GenResult<Vec<String>> {
    let mut out = Vec::new();
    if elements.len() > 1 {
        out.push("<>".to_owned());
        for e in elements {
            node(e, 1, &mut out)?;
        }
        out.push("</>".to_owned());
    } else {
        for e in elements {
            node(e, 0, &mut out)?;
        }
    }
    Ok(out)
}

/// An element with its `v-if` / `v-for` wrappers.
fn node(e: &Element, depth: usize, out: &mut Vec<String>) -> GenResult<()> {
    let t = translate(e)?;
    let pad = IND.repeat(depth);
    if let Some(each) = &t.each {
        let (item, list) = each.split_once(" in ").ok_or_else(|| {
            GenError::new(crate::tf!(
                "`v-for` needs `item in items`, got `{}`",
                "a `v-for`-hoz `elem in lista` kell, ez jött: `{}`",
                each
            ))
        })?;
        out.push(format!("{pad}{{{}.map(({}) => (", list.trim(), item.trim()));
        let inner = Translated {
            attrs: t.attrs,
            cond: t.cond,
            each: None,
        };
        wrapped(e, inner, depth + 1, out);
        out.push(format!("{pad}))}}"));
        return Ok(());
    }
    wrapped(e, t, depth, out);
    Ok(())
}

fn wrapped(e: &Element, t: Translated, depth: usize, out: &mut Vec<String>) {
    let pad = IND.repeat(depth);
    match &t.cond {
        Some(cond) => {
            out.push(format!("{pad}{{{cond} && ("));
            element(e, &t.attrs, depth + 1, out);
            out.push(format!("{pad})}}"));
        }
        None => element(e, &t.attrs, depth, out),
    }
}

/// The element itself; the children go through `node` again (their translation errors are collected lazily: a bad
/// child is reported by `validate` before the lines are written).
fn element(e: &Element, attrs: &[String], depth: usize, out: &mut Vec<String>) {
    let pad = IND.repeat(depth);
    let bare = e.children.is_empty() && e.text.is_none();
    let self_closing = bare && (is_component(&e.tag) || is_void(&e.tag));
    let one_line = format!(
        "{pad}<{}{}{}",
        e.tag,
        attrs.iter().map(|a| format!(" {a}")).collect::<String>(),
        if self_closing { " />" } else { ">" }
    );
    let wrapped = one_line.chars().count() > WRAP && attrs.len() > 1;
    if wrapped {
        out.push(format!("{pad}<{}", e.tag));
        out.extend(attrs.iter().map(|a| format!("{pad}{IND}{a}")));
        out.push(format!("{pad}{}", if self_closing { "/>" } else { ">" }));
    } else {
        out.push(one_line);
    }
    if self_closing {
        return;
    }
    let close = format!("</{}>", e.tag);
    if bare {
        if wrapped {
            out.push(format!("{pad}{close}"));
        } else if let Some(last) = out.last_mut() {
            last.push_str(&close);
        }
        return;
    }
    if e.children.is_empty() && !wrapped {
        let text = text_of(e.text.as_deref().unwrap_or(""));
        if !text.contains('\n') {
            if let Some(last) = out.last_mut() {
                last.push_str(&text);
                last.push_str(&close);
            }
            return;
        }
    }
    if let Some(text) = &e.text {
        for line in text_of(text).lines() {
            out.push(format!("{pad}{IND}{}", line.trim()));
        }
    }
    for child in &e.children {
        // a child that cannot be translated was rejected by `check` first
        let _ = node(child, depth + 1, out);
    }
    out.push(format!("{pad}{close}"));
}

/// Reject what the JSX emitter cannot translate (before any line is written).
pub fn check(elements: &[Element]) -> GenResult<()> {
    for e in elements {
        translate(e)?;
        if let Some(each) = e.attrs.iter().find(|a| a.name == "v-for") {
            let v = each.value.as_deref().unwrap_or("");
            if !v.contains(" in ") {
                return Err(GenError::new(crate::tf!(
                    "`v-for` needs `item in items`, got `{}`",
                    "a `v-for`-hoz `elem in lista` kell, ez jött: `{}`",
                    v
                )));
            }
        }
        check(&e.children)?;
    }
    Ok(())
}
