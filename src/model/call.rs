//! Hívás-él (függvény, metódus, statikus, `new`, konstans-elérés…).

use serde::{Deserialize, Serialize};

/// A statikusan fel nem oldható (futásidőben eldőlő) hívások kategóriája: nem hívó-él, csak jelölés.
pub const DYNAMIC_CALL: &str = "dynamic_call";

/// Az összes ismert hívás-kategória (a nyelvi frontendek kategorizálóinak közös listája): a `--category` ellenőrzése
/// és az „ismeretlen kategória” hibaüzenet ebből jön. Ábécében, a `dynamic_call` a végén (`category_rank`).
/// Új kategória a frontendben: ide is fel kell venni — a `known_categories_cover_every_frontend` teszt őrzi.
pub const KNOWN_CATEGORIES: [&str; 41] = [
    "anchor",
    "component_use",
    "composable_use",
    "extend",
    "external_call",
    "external_url",
    "framework_use",
    "function",
    "global_helper",
    "helper_use",
    "http_call",
    "i18n_helper",
    "image",
    "include",
    "inertia",
    "instantiation",
    "internal_file",
    "internal_helper",
    "interpolation",
    "macro_use",
    "model_use",
    "module_use",
    "native_go",
    "native_js",
    "native_php",
    "native_python",
    "native_rust",
    "native_swift",
    "orm_relation",
    "package_use",
    "route_helper",
    "static_call",
    "swiftui_api",
    "swiftui_modifier",
    "swiftui_view",
    "var",
    "vue_api",
    "vue_emit",
    "vue_macro",
    "vue_props",
    DYNAMIC_CALL,
];

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

#[cfg(test)]
mod tests {
    use super::*;

    fn call(category: &str, target: &str) -> Call {
        Call {
            container: None,
            caller: None,
            line: 1,
            end_line: None,
            kind: "x".into(),
            target: target.into(),
            resolved: None,
            on: None,
            category: category.into(),
            args: None,
            handler: None,
        }
    }

    #[test]
    fn dynamic_target_is_one_line_and_at_most_60_chars() {
        assert_eq!(Call::dynamic_target("$this->$m"), "$this->$m");
        assert_eq!(
            Call::dynamic_target("$obj\n    ->{$expr}"),
            "$obj ->{$expr}"
        );
        let exactly = "a".repeat(60);
        assert_eq!(Call::dynamic_target(&exactly), exactly);
        let long = "b".repeat(61);
        let cut = Call::dynamic_target(&long);
        assert_eq!(cut.chars().count(), 60);
        assert!(cut.ends_with('…'));
        // a többbájtos karakter nem vágódik félbe
        let accents = "é".repeat(80);
        assert_eq!(Call::dynamic_target(&accents).chars().count(), 60);
    }

    #[test]
    fn dynamic_category_sorts_last_and_is_detected() {
        assert!(call(DYNAMIC_CALL, "x").is_dynamic());
        assert!(!call("external_call", "x").is_dynamic());
        let mut cats = vec!["internal_helper", DYNAMIC_CALL, "external_call", "zzz_last"];
        cats.sort_by_key(|c| category_rank(c));
        assert_eq!(
            cats,
            vec!["internal_helper", "external_call", "zzz_last", DYNAMIC_CALL]
        );
    }
}
