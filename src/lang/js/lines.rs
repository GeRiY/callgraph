//! Bájt-offset → 1-alapú sorszám; a `.vue` script blokkjánál a blokk-eltolással.

/// Egy szövegrész sorindexe. `base_line` a szövegrész első sorának sorszáma a teljes fájlban.
#[derive(Debug, Clone)]
pub struct LineIndex {
    starts: Vec<usize>,
    base_line: u32,
}

impl LineIndex {
    /// Teljes fájl indexe (az első sor: 1).
    pub fn new(text: &str) -> Self {
        Self::with_base(text, 1)
    }

    /// Részlet indexe, amelynek első sora a fájl `base_line`. sora.
    pub fn with_base(text: &str, base_line: u32) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self { starts, base_line }
    }

    /// A részleten belüli `offset` bájt sora a teljes fájlban.
    pub fn line_of(&self, offset: usize) -> u32 {
        let idx = self.starts.partition_point(|&s| s <= offset) - 1;
        self.base_line + idx as u32
    }

    /// A részlet sorainak száma.
    pub fn line_count(&self) -> u32 {
        self.starts.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_offsets_to_lines() {
        let i = LineIndex::new("ab\ncd\n\nx");
        assert_eq!([0, 2, 3, 5, 6, 7].map(|o| i.line_of(o)), [1, 1, 2, 2, 3, 4]);
        assert_eq!(i.line_count(), 4);
    }

    #[test]
    fn applies_block_base_line() {
        let i = LineIndex::with_base("a\nb", 10);
        assert_eq!((i.line_of(0), i.line_of(2)), (10, 11));
    }
}
