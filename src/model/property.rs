//! Property (plain és promoted).

use serde::{Deserialize, Serialize, Serializer};

/// Egy konténer property-je.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Property {
    pub container: Option<String>,
    pub name: String,
    pub line: u32,
    #[serde(rename = "type")]
    pub type_hint: Option<String>,
    pub visibility: String,
    #[serde(rename = "static")]
    pub is_static: bool,
    pub readonly: bool,
    pub doc: Option<String>,
    /// JS/Vue-prop: a régi JS-motor alakja (`kind:'prop'`, `visibility`/`doc` nélkül).
    #[serde(skip, default)]
    pub js: bool,
}

impl Serialize for Property {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("Property", 8)?;
        st.serialize_field("container", &self.container)?;
        st.serialize_field("name", &self.name)?;
        st.serialize_field("line", &self.line)?;
        st.serialize_field("type", &self.type_hint)?;
        if !self.js {
            st.serialize_field("visibility", &self.visibility)?;
        }
        st.serialize_field("static", &self.is_static)?;
        st.serialize_field("readonly", &self.readonly)?;
        if self.js {
            st.serialize_field("kind", "prop")?;
        } else {
            st.serialize_field("doc", &self.doc)?;
        }
        st.end()
    }
}
