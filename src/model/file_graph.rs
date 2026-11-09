//! A fájl teljes gráfja: a JSONL-sor `graph` objektuma.

use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{
    Call, Comment, Constant, Container, ControlFlow, Method, ParseError, Property, StringLit, Write,
};

/// Egy PHP-fájl elemzésének eredménye (immutábilis; `FileGraphBuilder` állítja össze).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileGraph {
    pub file: String,
    /// JS/Vue: a fájl útja a projekt-gyökérhez képest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub framework: Option<String>,
    pub namespace: Option<String>,
    pub imports: Imports,
    pub containers: Vec<Container>,
    pub properties: Vec<Property>,
    pub constants: Vec<Constant>,
    pub methods: Vec<Method>,
    pub calls: Vec<Call>,
    pub comments: Vec<Comment>,
    #[serde(rename = "controlFlow")]
    pub control_flow: Vec<ControlFlow>,
    pub strings: Vec<StringLit>,
    pub writes: Vec<Write>,
    /// `.vue`: `{template:{line,endLine}|null, scriptSetup, styles}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sfc: Option<serde_json::Value>,
    /// Bináris tartalmú fájl (`lang::filetype::looks_binary`): üres gráf, a lekérdezések kihagyják. A
    /// projekt-gráf így megjegyzi, és a fájlt nem olvassa újra, amíg nem változik.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// A parser szintaktikai hibái (1-alapú sor + rövid üzenet); ha van, a vázlat hiányos lehet.
    #[serde(
        default,
        rename = "parseErrors",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub parse_errors: Vec<ParseError>,
}

impl FileGraph {
    /// A `c` container-é-e a `container` nevű, `line` sorú elem (metódus, hívás, property…). Alapból név szerint;
    /// ha egy azonos nevű container a másikba ágyazódik (JS/TS: a fájlnévről elnevezett `module X` és benne a
    /// `class X`), a belső tartománya a belsőé, a többi a külsőé — így minden elem pontosan egy alatt jelenik meg.
    /// Az egymásba nem ágyazott azonos nevűek (pl. több Rust `impl`) viselkedése változatlan.
    pub fn owns(&self, c: &Container, container: Option<&str>, line: u32) -> bool {
        if container != Some(c.name.as_str()) {
            return false;
        }
        let nested = |outer: &Container, inner: &Container| {
            !std::ptr::eq(outer, inner)
                && outer.line <= inner.line
                && inner.end_line <= outer.end_line
                && (outer.line, outer.end_line) != (inner.line, inner.end_line)
        };
        let inside = |x: &Container| x.line <= line && line <= x.end_line;
        let same: Vec<&Container> = self
            .containers
            .iter()
            .filter(|o| o.name == c.name)
            .collect();
        // egy belső, azonos nevű container tartományába esik → az övé
        if same.iter().any(|&o| nested(c, o) && inside(o)) {
            return false;
        }
        // ez a belső: csak a saját tartománya
        !same.iter().any(|&o| nested(o, c)) || inside(c)
    }
}

/// `alias => teljes név` import-térkép, forrásbeli sorrendben (JSON-objektumként szerializálva).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Imports(Vec<(String, String)>, bool);

impl Imports {
    /// Új import felvétele; azonos alias esetén az utolsó nyer (a JSON-objektum is így viselkedne).
    pub fn insert(&mut self, alias: String, full: String) {
        match self.0.iter_mut().find(|(a, _)| *a == alias) {
            Some(slot) => slot.1 = full,
            None => self.0.push((alias, full)),
        }
    }

    pub fn serialize_empty_as_object(&mut self) {
        self.1 = true;
    }

    pub fn get(&self, alias: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(a, _)| a == alias)
            .map(|(_, f)| f.as_str())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// A veszteségmentes (bináris) tároláshoz: a bejegyzések és az „üresen is objektum” jelző.
    pub fn parts(&self) -> (&[(String, String)], bool) {
        (&self.0, self.1)
    }

    /// A `parts` párja.
    pub fn from_parts(entries: Vec<(String, String)>, object: bool) -> Self {
        Self(entries, object)
    }
}

impl Serialize for Imports {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // A referencia (PHP json_encode) az üres térképet `[]`-ként írja.
        if self.0.is_empty() && !self.1 {
            return serializer.serialize_seq(Some(0))?.end();
        }
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (alias, full) in &self.0 {
            map.serialize_entry(alias, full)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Imports {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ImportsVisitor;
        impl<'de> Visitor<'de> for ImportsVisitor {
            type Value = Imports;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(crate::t!(
                    "an alias => full name object",
                    "alias => teljes név objektum"
                ))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Imports, A::Error> {
                Ok(Imports::default())
            }
            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Imports, A::Error> {
                let mut imports = Imports::default();
                while let Some((alias, full)) = access.next_entry::<String, String>()? {
                    imports.insert(alias, full);
                }
                Ok(imports)
            }
        }
        deserializer.deserialize_any(ImportsVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `golden/ref.jsonl` a crate-gyökértől felfelé keresve (a `work/<kód>/` munkapéldányokban is megtalálja).
    fn golden_ref() -> Option<String> {
        let mut dir = Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
        while let Some(d) = dir {
            if let Ok(s) = std::fs::read_to_string(d.join("golden/ref.jsonl")) {
                return Some(s);
            }
            dir = d.parent();
        }
        None
    }

    /// Minden referencia-sor `graph`-ja visszaolvasható és mezőegyezéssel újraírható.
    #[test]
    fn reference_rows_roundtrip() {
        let Some(reference) = golden_ref() else {
            eprintln!("golden/ref.jsonl nem található — a teszt kimarad");
            return;
        };
        let mut rows = 0;
        for line in reference.lines().filter(|l| !l.trim().is_empty()) {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let Some(graph) = row.get("graph") else {
                continue;
            };
            let parsed: FileGraph = serde_json::from_value(graph.clone())
                .unwrap_or_else(|e| panic!("{}: {e}", row["path"]));
            assert_eq!(
                &serde_json::to_value(&parsed).unwrap(),
                graph,
                "{}",
                row["path"]
            );
            rows += 1;
        }
        assert!(rows > 0);
    }

    #[test]
    fn imports_keep_source_order() {
        let src = r#"{"b":"X\\B","a":"X\\A"}"#;
        let i: Imports = serde_json::from_str(src).unwrap();
        assert_eq!(serde_json::to_string(&i).unwrap(), src);
    }
}
