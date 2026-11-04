//! Vue SFC felbontása legfelső szintű blokkokra (`<template>`, `<script>`, `<script setup>`, `<style>`).

use super::lines::LineIndex;

/// A blokk fajtája.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfcBlockKind {
    Template,
    Script,
    ScriptSetup,
    Style,
}

/// Egy SFC-blokk: a tartalom a fájl `[start, end)` bájtsávja.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcBlock {
    pub kind: SfcBlockKind,
    /// A `lang` attribútum értéke (pl. `ts`, `tsx`).
    pub lang: Option<String>,
    /// A tartalom kezdő bájt-offsetje a fájlban.
    pub start: usize,
    /// A tartalom záró bájt-offsetje (a záró tag előtt).
    pub end: usize,
    /// A nyitó tag sora (1-alapú).
    pub tag_line: u32,
    /// A tartalom első sora a fájlban (a nyitó tag `>` jelének sora).
    pub start_line: u32,
    /// A tartalom utolsó sora a fájlban (a záró tag sora).
    pub end_line: u32,
}

/// A felbontott `.vue` fájl blokkjai forrásbeli sorrendben.
#[derive(Debug, Clone, Default)]
pub struct Sfc {
    pub blocks: Vec<SfcBlock>,
}

impl Sfc {
    /// Felbontja a fájlt; a hibás / lezáratlan blokkot kihagyja.
    pub fn parse(src: &str) -> Self {
        let lines = LineIndex::new(src);
        let mut blocks = Vec::new();
        let mut pos = 0;
        while let Some(rel) = src[pos..].find('<') {
            let at = pos + rel;
            pos = at + 1;
            if src[at..].starts_with("<!--") {
                pos = src[at..].find("-->").map_or(src.len(), |e| at + e + 3);
                continue;
            }
            let Some((name, tag_end, attrs)) = open_tag(src, at) else {
                continue;
            };
            let Some(kind) = kind_of(&name, &attrs) else {
                continue;
            };
            let close = format!("</{name}");
            let Some(end) = find_close(src, tag_end, &close, &name) else {
                continue;
            };
            blocks.push(SfcBlock {
                kind,
                lang: attr_value(&attrs, "lang"),
                start: tag_end,
                end,
                tag_line: lines.line_of(at),
                start_line: lines.line_of(tag_end.saturating_sub(1)),
                end_line: lines.line_of(end),
            });
            pos = end;
        }
        Self { blocks }
    }

    /// A script blokkok (`<script>`, `<script setup>`) sorrendben.
    pub fn scripts(&self) -> impl Iterator<Item = &SfcBlock> {
        self.blocks
            .iter()
            .filter(|b| matches!(b.kind, SfcBlockKind::Script | SfcBlockKind::ScriptSetup))
    }

    /// Az első `<template>` blokk.
    pub fn template(&self) -> Option<&SfcBlock> {
        self.blocks
            .iter()
            .find(|b| b.kind == SfcBlockKind::Template)
    }
}

fn kind_of(name: &str, attrs: &str) -> Option<SfcBlockKind> {
    match name {
        "template" => Some(SfcBlockKind::Template),
        "style" => Some(SfcBlockKind::Style),
        "script" if has_flag(attrs, "setup") => Some(SfcBlockKind::ScriptSetup),
        "script" => Some(SfcBlockKind::Script),
        _ => None,
    }
}

/// A nyitó tag: (név, a `>` utáni offset, attribútum-szöveg); idézőjelek között a `>` nem zár.
fn open_tag(src: &str, at: usize) -> Option<(String, usize, String)> {
    let bytes = src.as_bytes();
    let name_start = at + 1;
    let mut i = name_start;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    let name = src[name_start..i].to_ascii_lowercase();
    let attrs_start = i;
    let mut quote = 0u8;
    while i < bytes.len() {
        match (quote, bytes[i]) {
            (0, b'"' | b'\'') => quote = bytes[i],
            (q, c) if q != 0 && c == q => quote = 0,
            (0, b'>') => return Some((name, i + 1, src[attrs_start..i].to_string())),
            _ => {}
        }
        i += 1;
    }
    None
}

/// A záró tag kezdete; a `<template>` egymásba ágyazását számolja.
fn find_close(src: &str, from: usize, close: &str, name: &str) -> Option<usize> {
    if name != "template" {
        return src[from..].find(close).map(|r| from + r);
    }
    let open = "<template";
    let (mut depth, mut i) = (1usize, from);
    loop {
        let next_close = src[i..].find(close)? + i;
        depth += src[i..next_close].matches(open).count();
        depth -= 1;
        if depth == 0 {
            return Some(next_close);
        }
        i = next_close + close.len();
    }
}

fn has_flag(attrs: &str, flag: &str) -> bool {
    attrs
        .split(|c: char| c.is_whitespace() || c == '=')
        .any(|t| t == flag)
}

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let at = attrs.find(&format!("{name}="))? + name.len() + 1;
    let rest = &attrs[at..];
    let value = match rest.chars().next()? {
        q @ ('"' | '\'') => rest[1..].split(q).next()?,
        _ => rest.split_whitespace().next()?,
    };
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_blocks_with_lines_and_lang() {
        let src = "<template>\n  <div>a > b</div>\n</template>\n<script setup lang=\"ts\">\nconst a = 1\n</script>\n<style>\n.a{}\n</style>\n";
        let sfc = Sfc::parse(src);
        let kinds: Vec<_> = sfc.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [
                SfcBlockKind::Template,
                SfcBlockKind::ScriptSetup,
                SfcBlockKind::Style
            ]
        );
        let s = sfc.scripts().next().unwrap();
        assert_eq!(s.lang.as_deref(), Some("ts"));
        assert_eq!((s.tag_line, s.start_line, s.end_line), (4, 4, 6));
        assert_eq!(&src[s.start..s.end], "\nconst a = 1\n");
    }

    #[test]
    fn nested_template_and_two_scripts() {
        let src = "<template><template v-if=\"a\">x</template></template>\n<script>\nlet a\n</script>\n<script setup>\nlet b\n</script>";
        let sfc = Sfc::parse(src);
        assert_eq!(sfc.template().unwrap().end_line, 1);
        assert_eq!(sfc.scripts().count(), 2);
    }
}
