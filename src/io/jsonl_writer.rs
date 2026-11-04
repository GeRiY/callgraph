//! JSONL-kiírás: soronként egy tömör JSON-objektum.

use std::io::Write;

use serde::Serialize;

use crate::error::Result;

/// Tömör JSON-sorokat ír egy `Write`-ba.
pub struct JsonlWriter<W: Write> {
    out: W,
}

impl<W: Write> JsonlWriter<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }

    /// Egy érték kiírása egy sorként, és azonnali flush (hogy a sor ne vesszen el hiba esetén).
    pub fn write_line<T: Serialize>(&mut self, value: &T) -> Result<()> {
        serde_json::to_writer(&mut self.out, value)?;
        self.out.write_all(b"\n")?;
        self.out.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_one_compact_line_per_value() {
        let mut buf = Vec::new();
        let mut w = JsonlWriter::new(&mut buf);
        w.write_line(&serde_json::json!({"a": 1})).unwrap();
        w.write_line(&serde_json::json!({"b": [1, 2]})).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "{\"a\":1}\n{\"b\":[1,2]}\n"
        );
    }
}
