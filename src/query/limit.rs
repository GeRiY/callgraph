//! A callgraph kompatibilis, alapértelmezett kimeneti korlát.

use std::env;
pub struct OutputLimit {
    pub limit: usize,
    pub full: bool,
}
impl OutputLimit {
    pub fn from_env(full: bool) -> Self {
        let value = env::var("CALLGRAPH_OUT_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(400);
        Self { limit: value, full }
    }
    pub fn method_index(&self, text: String, index: String) -> String {
        self.method_index_for("outline", text, index)
    }
    /// Egyfájlos tartalék: 400 sor fölött a metódus-index (a figyelmeztetés a tényleges formátumot nevezi meg).
    pub fn method_index_for(&self, format: &str, text: String, index: String) -> String {
        if self.full {
            return text;
        }
        let n = text.lines().count();
        if n <= self.limit {
            return text;
        }
        format!(
            "⚠ A --format {format} kimenete {n} sor lenne — helyette metódus-index. Egy metódus: <fájl>::<metódus> [--format outline|flow] vagy --body; a teljes kimenet: --full.\n\n{index}"
        )
    }
    pub fn directory_summary(
        &self,
        text: String,
        summary: String,
        format: &str,
        file_count: usize,
    ) -> String {
        if self.full || text.lines().count() <= self.limit || file_count <= 1 {
            return text;
        }
        format!(
            "⚠ A --format {format} kimenete {} sor lenne ({file_count} fájl) — helyette summary. Szűkíts: almappa, --grep <minta>, vagy egy fájl; a teljes kimenet: --full.\n\n{summary}",
            text.lines().count()
        )
    }
    pub fn with_value(mut self, value: Option<usize>) -> Self {
        if let Some(value) = value {
            self.limit = value;
        }
        self
    }
}
