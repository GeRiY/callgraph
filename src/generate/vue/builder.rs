//! Builder minta: a `.vue` fájl (`<script setup>`, `<template>`, `<style>`) és a composable-fájl (importok +
//! függvény) összeállítása lépésenként, a szakaszok között egy-egy üres sorral.

/// Az SFC építője; a szakaszok sorrendje fix: script, template, style.
#[derive(Debug, Default, Clone)]
pub struct SfcBuilder {
    script: Option<(bool, Vec<String>)>,
    template: Option<Vec<String>>,
    style: Option<(bool, Vec<String>)>,
}

impl SfcBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// `<script setup>` (`ts`: `lang="ts"`).
    pub fn script(mut self, ts: bool, lines: Vec<String>) -> Self {
        self.script = Some((ts, lines));
        self
    }

    pub fn template(mut self, lines: Vec<String>) -> Self {
        self.template = Some(lines);
        self
    }

    /// `<style scoped>` / `<style>`.
    pub fn style(mut self, scoped: bool, lines: Vec<String>) -> Self {
        self.style = Some((scoped, lines));
        self
    }

    /// A teljes fájl, záró újsorral.
    pub fn build(self) -> String {
        let mut blocks: Vec<Vec<String>> = Vec::new();
        if let Some((ts, lines)) = self.script {
            blocks.push(wrap(script_open(ts), "</script>", lines, false));
        }
        if let Some(lines) = self.template {
            blocks.push(wrap("<template>", "</template>", lines, true));
        }
        if let Some((scoped, lines)) = self.style {
            let open = if scoped { "<style scoped>" } else { "<style>" };
            blocks.push(wrap(open, "</style>", lines, false));
        }
        let mut text = blocks
            .iter()
            .map(|b| b.join("\n"))
            .collect::<Vec<_>>()
            .join("\n\n");
        text.push('\n');
        text
    }
}

/// A script nyitó tagje.
pub fn script_open(ts: bool) -> &'static str {
    if ts {
        "<script setup lang=\"ts\">"
    } else {
        "<script setup>"
    }
}

/// A style nyitó tagje.
pub fn style_open(scoped: bool) -> &'static str {
    if scoped { "<style scoped>" } else { "<style>" }
}

/// Nyitó tag + sorok + záró tag; `indent`: a tartalom egy szinttel beljebb (template).
fn wrap(open: &str, close: &str, lines: Vec<String>, indent: bool) -> Vec<String> {
    let mut out = vec![open.to_owned()];
    out.extend(lines.into_iter().map(|l| {
        if indent && !l.is_empty() {
            format!("  {l}")
        } else {
            l
        }
    }));
    out.push(close.to_owned());
    out
}

/// A composable-fájl: importok, üres sor, a függvény sorai; záró újsorral.
pub fn composable_file(imports: &[String], function: &[String]) -> String {
    let mut lines: Vec<String> = imports.to_vec();
    if !imports.is_empty() {
        lines.push(String::new());
    }
    lines.extend(function.iter().cloned());
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// A script-szakasz tartalma: importok, üres sor, tagok.
pub fn script_body(imports: &[String], members: &[String]) -> Vec<String> {
    let mut lines: Vec<String> = imports.to_vec();
    if !imports.is_empty() && !members.is_empty() {
        lines.push(String::new());
    }
    lines.extend(members.iter().cloned());
    lines
}
