//! A kimenet sorainak olvasása a szabályokhoz: metódus-sorok (`  név()  L10-30`) és azonosítók.

/// Egy metódus-sor az outline-ban / a metódus-indexben / a hívó-találatban: `  név()  L10-30`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MethodLine {
    pub name: String,
    pub start: u32,
    pub end: u32,
}

/// A metódus-sorok (két szóközös behúzás, `név()  L<a>-<b>`).
pub(crate) fn method_lines(output: &str) -> Vec<MethodLine> {
    output.lines().filter_map(method_line).collect()
}

pub(crate) fn method_line(line: &str) -> Option<MethodLine> {
    let rest = line.strip_prefix("  ")?;
    if rest.starts_with(' ') {
        return None;
    }
    let (name, tail) = rest.split_once("()  L")?;
    if !is_identifier(name) {
        return None;
    }
    let range = tail.split(|c: char| c.is_whitespace()).next()?;
    let (start, end) = range.split_once('-')?;
    Some(MethodLine {
        name: name.to_owned(),
        start: start.parse().ok()?,
        end: end.parse().ok()?,
    })
}

/// Egy (behúzás nélküli) locate-indoklás metódusneve: `név()  L…`.
pub(crate) fn method_name(line: &str) -> Option<String> {
    let (name, _) = line.split_once("()  L")?;
    is_identifier(name).then(|| name.to_owned())
}

fn is_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}
