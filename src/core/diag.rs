//! Diagnosztika-gyűjtő: a webes kötések elve „ami nem értelmezhető, kimarad — soha nem hiba”, de a csendes kimaradás
//! a felhasználó felé eltünteti a hiányt (a nézet „nincs találat”-ot mond). A kihagyás itt ok-kóddal és `fájl:sor`
//! helyével gyűlik, a nézetek lábléce összesíti. Nem hiba, nem kilépési kód — csak látszik.

use crate::i18n::Lang;

/// Egy kihagyott elem: ok-kód (csoportosításhoz), kétnyelvű ok-szöveg és a hely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    /// Az ok `(angol, magyar)` szövege, `1 form skipped: …` alakú mondat-folytatás nélkül.
    pub text: (&'static str, &'static str),
    pub file: String,
    pub line: u32,
}

/// A diagnosztikák gyűjtője.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

/// Egy láblécsor legfeljebb ennyi helyet sorol fel, a többi `+N`.
const MAX_SITES: usize = 3;

impl Diagnostics {
    /// Egy kihagyás felvétele.
    pub fn push(
        &mut self,
        code: &'static str,
        text: (&'static str, &'static str),
        file: &str,
        line: u32,
    ) {
        self.items.push(Diagnostic {
            code,
            text,
            file: file.to_owned(),
            line,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// A láblécsorok a `scope` fájljaira (üres `scope`: nincs szűrés): kódonként egy sor,
    /// `? 2 skipped: <ok> (a.vue L3, b.vue L9)`; kód szerint rendezve, a helyek fájl / sor szerint.
    pub fn footer(&self, scope: &[String], lang: Lang) -> Vec<String> {
        let mut codes: Vec<&'static str> = self.items.iter().map(|d| d.code).collect();
        codes.sort_unstable();
        codes.dedup();
        codes
            .into_iter()
            .filter_map(|code| {
                let mut group: Vec<&Diagnostic> = self
                    .items
                    .iter()
                    .filter(|d| d.code == code && (scope.is_empty() || scope.contains(&d.file)))
                    .collect();
                if group.is_empty() {
                    return None;
                }
                group.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
                group.dedup_by(|a, b| (&a.file, a.line) == (&b.file, b.line));
                let sites: Vec<String> = group
                    .iter()
                    .take(MAX_SITES)
                    .map(|d| format!("{} L{}", d.file, d.line))
                    .collect();
                let more = group.len().saturating_sub(MAX_SITES);
                let more = if more > 0 {
                    format!(", +{more}")
                } else {
                    String::new()
                };
                let reason = lang.pick(group[0].text.0, group[0].text.1);
                let n = group.len();
                Some(crate::tfl!(
                    lang,
                    "? {n} skipped: {reason} ({}{more})",
                    "? {n} kihagyva: {reason} ({}{more})",
                    sites.join(", ")
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHY: (&str, &str) = ("no controller action", "nincs controller-akció");

    #[test]
    fn footer_groups_by_code_and_lists_sites() {
        let mut d = Diagnostics::default();
        assert!(d.is_empty());
        d.push("form-action", WHY, "b.vue", 9);
        d.push("form-action", WHY, "a.vue", 3);
        d.push("other", ("x", "y"), "a.vue", 1);
        let all = d.footer(&[], Lang::En);
        assert_eq!(
            all,
            [
                "? 2 skipped: no controller action (a.vue L3, b.vue L9)",
                "? 1 skipped: x (a.vue L1)"
            ]
        );
        let scoped = d.footer(&["b.vue".to_owned()], Lang::Hu);
        assert_eq!(scoped, ["? 1 kihagyva: nincs controller-akció (b.vue L9)"]);
        d.clear();
        assert!(d.footer(&[], Lang::En).is_empty());
    }

    #[test]
    fn footer_caps_the_sites() {
        let mut d = Diagnostics::default();
        for line in 1..=5 {
            d.push("c", WHY, "a.vue", line);
        }
        let out = d.footer(&[], Lang::En);
        assert_eq!(
            out,
            ["? 5 skipped: no controller action (a.vue L1, a.vue L2, a.vue L3, +2)"]
        );
    }
}
