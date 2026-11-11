//! Hívás-él (függvény, metódus, statikus, `new`, konstans-elérés…).

use serde::{Deserialize, Serialize};

/// A statikusan fel nem oldható (futásidőben eldőlő) hívások kategóriája: nem hívó-él, csak jelölés.
pub const DYNAMIC_CALL: &str = "dynamic_call";

/// A `Call.kind` (`type`) a dinamikus hívásoknál.
pub const DYNAMIC_KIND: &str = "dynamic";

/// A kategória helye a kimeneti listákban: a `dynamic_call` mindig a lista végére kerül (a többi marad ábécében).
pub fn category_rank(category: &str) -> u8 {
    u8::from(category == DYNAMIC_CALL)
}

/// Egy hívás a `caller` metódusban (`__top__` = osztály-szint).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub container: Option<String>,
    pub caller: Option<String>,
    pub line: u32,
    #[serde(rename = "endLine", default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(rename = "type")]
    pub kind: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// JS template: puszta függvénynév-eseménykezelő.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<bool>,
}

impl Call {
    /// Dinamikus hívás-jelölés (`dynamic_call`): a cél a hívott kifejezés szövege, nincs feloldás.
    pub fn is_dynamic(&self) -> bool {
        self.category == DYNAMIC_CALL
    }

    /// A hívott kifejezés szövege a jelöléshez: egy sorra vágva, ~60 karakterre rövidítve.
    pub fn dynamic_target(expr: &str) -> String {
        let one_line = expr.split_whitespace().collect::<Vec<_>>().join(" ");
        if one_line.chars().count() <= 60 {
            return one_line;
        }
        let cut: String = one_line.chars().take(59).collect();
        format!("{cut}…")
    }
}
