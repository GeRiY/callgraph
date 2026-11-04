//! Osztálykonstans / enum case.

use serde::{Deserialize, Serialize, Serializer};

/// Egy konténer konstansa (`kind`: const | case).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Constant {
    pub container: Option<String>,
    pub kind: String,
    pub name: String,
    pub line: u32,
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip)]
    pub caller: Option<String>,
    pub doc: Option<String>,
    /// JS-konstans / emit: a régi JS-motor alakja (`doc` nélkül).
    #[serde(skip, default)]
    pub js: bool,
}

impl Serialize for Constant {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let css = self.kind == "custom-property";
        let scss = self.kind == "scss-variable";
        let mut state = serializer.serialize_struct("Constant", if css { 8 } else { 7 })?;
        state.serialize_field("container", &self.container)?;
        if css || scss {
            state.serialize_field("caller", &self.caller)?;
        }
        state.serialize_field("kind", &self.kind)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("line", &self.line)?;
        state.serialize_field("value", &self.value)?;
        if let Some(visibility) = &self.visibility {
            state.serialize_field("visibility", visibility)?;
        }
        if !css && !scss && !self.js {
            state.serialize_field("doc", &self.doc)?;
        }
        state.end()
    }
}
