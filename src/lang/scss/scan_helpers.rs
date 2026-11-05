use crate::model::{Call, Constant, ControlFlow, FileGraphBuilder};

pub(super) fn call(
    container: &str,
    line: u32,
    kind: &str,
    target: &str,
    category: &str,
    args: Vec<String>,
) -> Call {
    Call {
        handler: None,
        container: Some(container.into()),
        caller: None,
        line,
        end_line: None,
        kind: kind.into(),
        target: target.into(),
        resolved: None,
        on: None,
        category: category.into(),
        args: Some(args),
    }
}

pub(super) fn declaration(s: &str) -> Option<(&str, String, Vec<String>)> {
    for kind in ["@mixin", "@function"] {
        if let Some(x) = s.strip_prefix(kind) {
            let (name, args) = call_parts(x.trim())?;
            return Some((&kind[1..], name.into(), args));
        }
    }
    None
}

pub(super) fn call_parts(s: &str) -> Option<(&str, Vec<String>)> {
    let open = s.find('(')?;
    let close = s[open + 1..].find(')')? + open + 1;
    Some((
        s[..open].trim(),
        s[open + 1..close]
            .split(',')
            .map(|x| x.trim().into())
            .filter(|x: &String| !x.is_empty())
            .collect(),
    ))
}

pub(super) fn functions(s: &str) -> Vec<(&str, Vec<String>)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if let Some(open) = s[..i].rfind("#{")
            && s[open + 2..i].find('}').is_none()
        {
            i += 1;
            continue;
        }
        if b[i].is_ascii_alphabetic() || b[i] == b'-' {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'-') {
                j += 1;
            }
            if j < b.len()
                && b[j] == b'('
                && let Some((n, a)) = call_parts(&s[i..])
            {
                out.push((n, a));
                i = j;
            }
        }
        i += 1;
    }
    out
}

pub(super) fn is_rule(s: &str) -> bool {
    (s.starts_with('.') || s.starts_with('#') || s.starts_with('%') || s.starts_with('&'))
        && !s.ends_with(':')
}

/// Változó-deklaráció (`$név: érték;`) a sor elején. A `@mixin` / `@function` paraméterlistája
/// (`$color: t.$brand, …`) és az `@include` kulcsszavas argumentuma nem konstans; a többsoros paraméterlista
/// folytatósorait a hívó zárja ki (`paren_depth`).
pub(super) fn constant(builder: &mut FileGraphBuilder, container: &str, raw: &str, ln: u32) {
    let s = raw.trim();
    if !s.starts_with('$') {
        return;
    }
    let Some(pos) = s.find('$') else { return };
    let Some((pos, name_end)) = s[pos..]
        .match_indices('$')
        .map(|(offset, _)| pos + offset)
        .find_map(|start| {
            let end = s[start + 1..]
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                .map(|x| start + 1 + x)
                .unwrap_or(s.len());
            s[end..]
                .trim_start()
                .starts_with(':')
                .then_some((start, end))
        })
    else {
        return;
    };
    let value = s[name_end..]
        .trim_start_matches(':')
        .trim()
        .trim_end_matches(';');
    builder.constant(Constant {
        js: false,
        container: Some(container.into()),
        caller: None,
        kind: "scss-variable".into(),
        name: s[pos..name_end].into(),
        line: ln,
        value: Some(value.into()),
        visibility: None,
        doc: None,
    });
}

/// A nyitott zárójelek mélységének változása egy sorban (idézőjeles szövegen és a sorvégi ` // megjegyzés`-en
/// kívül; az `url(http://…)` `//`-je nem megjegyzés, mert nem szóköz / sorkezdet előzi meg).
pub(super) fn paren_delta(raw: &str) -> isize {
    let mut delta = 0;
    let mut quote = None;
    let (mut prev, mut before) = (' ', ' ');
    for c in raw.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '/' if prev == '/' && before.is_whitespace() => break,
                '(' => delta += 1,
                ')' => delta -= 1,
                _ => {}
            },
        }
        (before, prev) = (prev, c);
    }
    delta
}

pub(super) fn control_flow(builder: &mut FileGraphBuilder, container: &str, raw: &str, ln: u32) {
    for kind in ["if", "each", "for", "while", "media", "supports"] {
        let prefix = format!("@{kind}");
        if let Some(start) = raw.find(&prefix)
            && raw[start + prefix.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || c == '(')
        {
            let end = raw.find('{').unwrap_or(raw.len());
            builder.control_flow(ControlFlow {
                container: Some(container.into()),
                caller: None,
                kind: kind.into(),
                line: ln,
                cond: Some(raw[start + prefix.len()..end].trim_start().into()),
                branches: Vec::new(),
            });
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::scanner::ScssScanner;
    use std::sync::mpsc;
    use std::time::Duration;

    fn sample(name: &str) -> String {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("golden/samples/scss");
        std::fs::read_to_string(dir.join(name)).expect("SCSS golden sample")
    }

    fn constants(name: &str, source: &str) -> Vec<(String, String)> {
        let graph = serde_json::to_value(ScssScanner::new(name, source).scan().graph).unwrap();
        graph["constants"]
            .as_array()
            .map(|list| {
                list.iter()
                    .map(|c| {
                        let text = |k: &str| c[k].as_str().unwrap_or_default().to_owned();
                        (text("name"), text("value"))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn mixin_and_function_parameters_are_not_constants() {
        // regresszió: a `@mixin button($color: t.$brand, $radius: 4px) {` sora `$color` konstans lett
        assert!(constants("_mixins.scss", &sample("_mixins.scss")).is_empty());
        assert!(constants("_helpers.scss", &sample("_helpers.scss")).is_empty());
        assert!(constants("theme.scss", &sample("theme.scss")).is_empty());
        let graph = serde_json::to_value(
            ScssScanner::new("_mixins.scss", &sample("_mixins.scss"))
                .scan()
                .graph,
        )
        .unwrap();
        let button = graph["methods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["name"] == "button")
            .expect("button mixin");
        assert_eq!(
            button["params"],
            serde_json::json!(["$color: t.$brand", "$radius: 4px"])
        );

        let multi = "@mixin card(\n  $pad: 4px,\n  $bg: url(http://x/y.png)\n) {\n  padding: $pad;\n}\n$after: 1px;\n";
        assert_eq!(
            constants("multi.scss", multi),
            vec![("$after".to_owned(), "1px".to_owned())]
        );
    }

    #[test]
    fn variable_declarations_are_still_constants() {
        assert_eq!(
            constants("_tokens.scss", &sample("_tokens.scss")),
            vec![
                ("$brand".to_owned(), "#2f6fed".to_owned()),
                (
                    "$spacing".to_owned(),
                    "(sm: 4px, md: 8px, lg: 16px)".to_owned()
                ),
                (
                    "$breakpoints".to_owned(),
                    "(tablet: 768px, desktop: 1200px)".to_owned()
                ),
            ]
        );
        let src = ".a {\n  $local: 2px; // note (x\n  margin: $local;\n}\n$bg: url(http://h/a.png);\n$top: 3;\n";
        let names: Vec<String> = constants("x.scss", src).into_iter().map(|c| c.0).collect();
        assert_eq!(names, ["$local", "$bg", "$top"]);
    }

    #[test]
    fn all_golden_samples_finish() {
        for name in [
            "_helpers.scss",
            "_mixins.scss",
            "_placeholders.scss",
            "_tokens.scss",
            "forms.scss",
            "index.scss",
            "layout.scss",
            "theme.scss",
        ] {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .map(|d| d.join("golden/samples/scss"))
                .find(|d| d.is_dir())
                .expect("golden/samples/scss");
            let source = std::fs::read_to_string(dir.join(name)).expect("SCSS golden sample");
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(ScssScanner::new(name, &source).scan());
            });
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap_or_else(|_| panic!("scanner stalled on {name}"));
        }
    }
}
