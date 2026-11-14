//! A template-emitter: az `Element`-fából behúzott HTML-sorok (2 szóköz), a tag-ellenőrző (kiegyensúlyozott
//! nyitó / záró tagek) és a PascalCase komponensek kigyűjtése az automatikus importhoz.

use std::collections::BTreeSet;

use super::model::Element;

const IND: &str = "  ";
/// Ennél hosszabb nyitó taget az attribútumok soronkénti törésével írunk.
const WRAP: usize = 100;

/// A záró tag nélküli (void) HTML-elemek.
const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// A beépített / globálisan regisztrált nagybetűs elemek: nem importálandó komponensek.
const BUILTIN: [&str; 8] = [
    "Transition",
    "TransitionGroup",
    "KeepAlive",
    "Teleport",
    "Suspense",
    "Component",
    "RouterLink",
    "RouterView",
];

/// PascalCase tag = komponens.
pub fn is_component(tag: &str) -> bool {
    tag.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// A fában használt, importra szoruló komponens-nevek.
pub fn components(elements: &[Element]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect(elements, &mut out);
    out
}

fn collect(elements: &[Element], out: &mut BTreeSet<String>) {
    for e in elements {
        if is_component(&e.tag) && !BUILTIN.contains(&e.tag.as_str()) {
            out.insert(e.tag.clone());
        }
        collect(&e.children, out);
    }
}

fn attr_text(name: &str, value: Option<&str>) -> String {
    match value {
        None => name.to_owned(),
        Some(v) if v.contains('"') => format!("{name}='{v}'"),
        Some(v) => format!("{name}=\"{v}\""),
    }
}

/// Az elem attribútumai kiírási sorrendben: `class` (az Emmet-osztályok + az explicit `class`), `id`, a többi.
fn attributes(e: &Element) -> Vec<String> {
    let mut classes: Vec<String> = e.classes.clone();
    let mut rest = Vec::new();
    for a in &e.attrs {
        let class_value = if a.name == "class" {
            a.value.as_ref()
        } else {
            None
        };
        match class_value {
            Some(v) => classes.extend(v.split_whitespace().map(str::to_owned)),
            None => rest.push(attr_text(&a.name, a.value.as_deref())),
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
pub fn lines(elements: &[Element]) -> Vec<String> {
    let mut out = Vec::new();
    for e in elements {
        element(e, 0, &mut out);
    }
    out
}

fn element(e: &Element, depth: usize, out: &mut Vec<String>) {
    let pad = IND.repeat(depth);
    let attrs = attributes(e);
    let bare = e.children.is_empty() && e.text.is_none();
    let self_closing =
        bare && (is_component(&e.tag) || e.tag == "slot" || VOID.contains(&e.tag.as_str()));
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
        let text = e.text.as_deref().unwrap_or("");
        if !text.contains('\n') {
            if let Some(last) = out.last_mut() {
                last.push_str(text);
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
    for child in &e.children {
        element(child, depth + 1, out);
    }
    out.push(format!("{pad}{close}"));
}

/// Kiegyensúlyozott-e a tagek nyitása / zárása a szövegben: az első hiba (bájt-eltolás, üzenet).
pub fn check_tags(text: &str) -> Result<(), (usize, String)> {
    let bytes = text.as_bytes();
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"{{") {
            i = text[i..].find("}}").map_or(bytes.len(), |e| i + e + 2);
            continue;
        }
        if bytes[i..].starts_with(b"<!--") {
            i = text[i..].find("-->").map_or(bytes.len(), |e| i + e + 3);
            continue;
        }
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let closing = bytes.get(i + 1) == Some(&b'/');
        let name_start = i + 1 + usize::from(closing);
        let name_end = text[name_start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')))
            .map_or(bytes.len(), |e| name_start + e);
        let name = &text[name_start..name_end];
        if name.is_empty() {
            i += 1;
            continue;
        }
        // a tag vége: idézőjelek között a `>` nem zár
        let mut j = name_end;
        let mut quote = 0u8;
        while j < bytes.len() {
            match (quote, bytes[j]) {
                (0, b'"' | b'\'') => quote = bytes[j],
                (q, c) if q != 0 && c == q => quote = 0,
                (0, b'>') => break,
                _ => {}
            }
            j += 1;
        }
        if j >= bytes.len() {
            return Err((i, crate::tf!("unclosed `<{}`", "lezáratlan `<{}`", name)));
        }
        let self_closed = bytes[j - 1] == b'/';
        if closing {
            match stack.pop() {
                Some((open, _)) if open == name => {}
                Some((open, _)) => {
                    return Err((
                        i,
                        crate::tf!(
                            "`</{}>` closes `<{}>`",
                            "a `</{}>` a `<{}>` taget zárja",
                            name,
                            open
                        ),
                    ));
                }
                None => {
                    return Err((
                        i,
                        crate::tf!(
                            "`</{}>` has no opening tag",
                            "a `</{}>`-nek nincs nyitó tagje",
                            name
                        ),
                    ));
                }
            }
        } else if !self_closed && !VOID.contains(&name) {
            stack.push((name.to_owned(), i));
        }
        i = j + 1;
    }
    match stack.pop() {
        Some((open, at)) => Err((
            at,
            crate::tf!("`<{}>` is never closed", "a `<{}>` nincs lezárva", open),
        )),
        None => Ok(()),
    }
}

/// Void (záró tag nélküli) HTML-elem-e a tag; kis- és nagybetű-érzékeny: a Vue-ban a `Link` komponens, nem a `<link>`.
pub fn is_void(tag: &str) -> bool {
    VOID.contains(&tag)
}

/// A sor egy olyan nyitó taget tartalmaz-e, amelyhez a későbbi sorokban jön a záró tag (a következő sor gyerek).
pub fn opens_element(line: &str) -> bool {
    let t = line.trim();
    if !t.starts_with('<') || t.starts_with("</") || t.starts_with("<!--") {
        return false;
    }
    if t.ends_with("/>") || !t.ends_with('>') || t.contains("</") {
        return false;
    }
    let name: String = t[1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
        .collect();
    !name.is_empty() && !is_void(&name)
}
