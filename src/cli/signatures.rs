//! `--signatures`: a normál outline (és a `--grep` hívó-outline) a metódussorban a szignatúrát írja ki
//! (`updateOrders(array $ids, bool $notify = false): array  L4-9`) a `name()` helyett. Környezeti megfelelője:
//! `CALLGRAPH_SIGNATURES=1`. Alapból a kimenet változatlan.

/// A kapcsoló neve.
pub const SIGNATURES_FLAG: &str = "--signatures";

/// A környezeti változó (`1` / `true` / `yes` / `on`: be); a belépő a kapcsolóra is ezt állítja be.
pub const SIGNATURES_ENV: &str = "CALLGRAPH_SIGNATURES";

/// A globális `--signatures` kapcsoló kivétele az argumentumok közül (az első `--` előtt); `true`, ha szerepelt.
pub fn take_signatures(args: &mut Vec<String>) -> bool {
    let end = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let before = args.len();
    let mut at = 0;
    args.retain(|a| {
        at += 1;
        at > end || a != SIGNATURES_FLAG
    });
    args.len() != before
}

/// Be van-e kapcsolva (`CALLGRAPH_SIGNATURES`).
pub fn enabled() -> bool {
    std::env::var(SIGNATURES_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_flag_before_the_separator_only() {
        let mut args: Vec<String> = ["a.php", "--signatures", "--", "--signatures"]
            .map(str::to_owned)
            .to_vec();
        assert!(take_signatures(&mut args));
        assert_eq!(args, ["a.php", "--", "--signatures"]);
    }
}
