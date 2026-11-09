//! `--strict` (CI-hez): a `show` (fájl, mappa, `--grep`), a `locate` és a `profile` 3-as kilépési kóddal jelzi, ha
//! egy beolvasott fájl szintaktikai hibás; a kimenet változatlan. Alapból a hibás fájl csak `⚠` sor, a kilépési kód
//! 0 marad (szándékos: a részleges térkép is használható). Környezeti megfelelője: `CALLGRAPH_STRICT=1`. Csak a
//! CLI-n (az MCP-szerver nem nézi).

/// A kapcsoló neve.
pub const STRICT_FLAG: &str = "--strict";

/// A környezeti változó (`1` / `true` / `yes` / `on`: be); a belépő a `--strict` kapcsolóra is ezt állítja be.
pub const STRICT_ENV: &str = "CALLGRAPH_STRICT";

/// A kilépési kód szigorú módban, ha egy beolvasott fájl szintaktikai hibás (2: használati hiba, 1: belső hiba).
pub const STRICT_EXIT_CODE: i32 = 3;

/// A globális `--strict` kapcsoló kivétele az argumentumok közül (bárhol, az első `--` előtt); `true`, ha szerepelt.
pub fn take_strict(args: &mut Vec<String>) -> bool {
    let end = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let before = args.len();
    let mut at = 0;
    args.retain(|a| {
        at += 1;
        at > end || a != STRICT_FLAG
    });
    args.len() != before
}

/// Be van-e kapcsolva a szigorú mód (`CALLGRAPH_STRICT`).
pub(crate) fn enabled() -> bool {
    std::env::var(STRICT_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// A sikeres futás kilépési kódja: szigorú módban 3, ha volt szintaktikai hibás fájl, különben 0.
pub(crate) fn exit_code(broken: bool) -> i32 {
    if broken && enabled() {
        STRICT_EXIT_CODE
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_flag_before_the_separator_only() {
        let mut args: Vec<String> = ["show", "a.js", "--strict", "--", "--strict"]
            .map(str::to_owned)
            .to_vec();
        assert!(take_strict(&mut args));
        assert_eq!(args, ["show", "a.js", "--", "--strict"]);
        let mut none: Vec<String> = vec!["show".into()];
        assert!(!take_strict(&mut none));
    }
}
