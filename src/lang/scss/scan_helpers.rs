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

pub(super) fn constant(builder: &mut FileGraphBuilder, container: &str, raw: &str, ln: u32) {
    let s = raw.trim();
    if !(s.starts_with('$')
        || s.starts_with("@function")
        || s.starts_with("@mixin")
        || s.starts_with("@include"))
    {
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
