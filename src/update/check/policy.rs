//! A háttér-ellenőrzés döntései tiszta függvényekként (állapot, idő, saját verzió, környezet) — I/O nélkül,
//! így teljesen unit-tesztelhetők: be van-e kapcsolva, kell-e indítani a háttér-ellenőrzést, szóljunk-e.

use super::state::CheckState;
use crate::update::Version;

/// Ennyi időnként ellenőrzünk, és ugyanarról a verzióról legfeljebb ennyi időnként szólunk (24 óra).
pub(crate) const INTERVAL_SECS: u64 = 24 * 60 * 60;

/// A bekapcsolást befolyásoló környezet.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CheckEnv {
    /// A `CALLGRAPH_UPDATE_CHECK` értéke (`0` / `off` / `false` / `no` kikapcsol).
    pub(crate) setting: Option<String>,
    /// Be van-e állítva a `CI` (CI-ben nincs ellenőrzés).
    pub(crate) ci: bool,
    /// Skill-telepítésben fut-e a bináris (`InstallTarget`); fejlesztői buildnél nincs ellenőrzés.
    pub(crate) in_skill: bool,
}

impl CheckEnv {
    /// Be van-e kapcsolva az automatikus ellenőrzés.
    pub(crate) fn enabled(&self) -> bool {
        let opted_out = self.setting.as_deref().is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "off" | "false" | "no"
            )
        });
        !opted_out && !self.ci && self.in_skill
    }
}

/// A futás utáni döntés: az új állapot (mentendő, ha változott), indítsunk-e háttér-ellenőrzést, és miről szóljunk.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Decision {
    pub(crate) state: CheckState,
    pub(crate) spawn: bool,
    pub(crate) notice: Option<Version>,
}

/// Régebbi-e `then` legalább egy intervallummal `now`-nál (a visszaállított óra is lejártnak számít).
fn expired(then: u64, now: u64) -> bool {
    then.abs_diff(now) >= INTERVAL_SECS
}

/// A döntés egy sikeres (`succeeded`) vagy sikertelen parancs után. Az ellenőrzés indításakor a `checked_at`
/// azonnal `now` lesz (a párhuzamos futások ne indítsanak még egyet); értesítés csak sikeres parancs után,
/// ha a `latest` újabb a sajátnál, és erről a verzióról 24 órája nem szóltunk.
pub(crate) fn decide(state: &CheckState, now: u64, current: &Version, succeeded: bool) -> Decision {
    let mut next = state.clone();
    let spawn = expired(state.checked_at, now);
    if spawn {
        next.checked_at = now;
    }
    let notice = state
        .latest
        .as_deref()
        .and_then(Version::parse)
        .filter(|latest| succeeded && latest.supersedes(current))
        .filter(|latest| {
            let same =
                state.notified_version.as_deref().and_then(Version::parse) == Some(latest.clone());
            !same || state.notified_at.is_none_or(|at| expired(at, now))
        });
    if let Some(latest) = &notice {
        next.notified_at = Some(now);
        next.notified_version = Some(latest.to_string());
    }
    Decision {
        state: next,
        spawn,
        notice,
    }
}

/// Az MCP `--auto-update` indításkori döntése: `Some(új állapot)` = próbáljuk meg (az állapotot a hálózat előtt
/// menteni kell, hogy a párhuzamosan induló szerverek ne próbálkozzanak újra), `None` = most nem.
/// Naponta legfeljebb egy kísérlet (`auto_update_at`); azon belül is csak akkor, ha a gyorsítótár (`checked_at`,
/// `latest`) lejárt, vagy már tudjuk, hogy van újabb stabil verzió.
pub(crate) fn decide_auto(state: &CheckState, now: u64, current: &Version) -> Option<CheckState> {
    if state.auto_update_at.is_some_and(|at| !expired(at, now)) {
        return None;
    }
    let newer = state
        .latest
        .as_deref()
        .and_then(Version::parse)
        .is_some_and(|latest| latest.supersedes(current));
    if !newer && !expired(state.checked_at, now) {
        return None;
    }
    Some(CheckState {
        checked_at: now,
        auto_update_at: Some(now),
        ..state.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = INTERVAL_SECS;
    const NOW: u64 = 1_800_000_000;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn state(checked_at: u64, latest: Option<&str>) -> CheckState {
        CheckState {
            checked_at,
            latest: latest.map(str::to_owned),
            ..CheckState::default()
        }
    }

    #[test]
    fn enabled_only_in_a_skill_install_without_opt_out_or_ci() {
        let on = CheckEnv {
            setting: None,
            ci: false,
            in_skill: true,
        };
        assert!(on.enabled());
        for off in ["0", "off", "OFF", "false", "no", " False "] {
            let env = CheckEnv {
                setting: Some(off.into()),
                ..on.clone()
            };
            assert!(!env.enabled(), "{off}");
        }
        for still_on in ["1", "on", "", "yes"] {
            let env = CheckEnv {
                setting: Some(still_on.into()),
                ..on.clone()
            };
            assert!(env.enabled(), "{still_on}");
        }
        assert!(
            !CheckEnv {
                ci: true,
                ..on.clone()
            }
            .enabled()
        );
        assert!(
            !CheckEnv {
                in_skill: false,
                ..on
            }
            .enabled()
        );
    }

    #[test]
    fn spawns_when_missing_or_older_than_a_day_and_stamps_checked_at() {
        let current = v("0.1.0");
        let d = decide(&CheckState::default(), NOW, &current, true);
        assert!(d.spawn);
        assert_eq!(d.state.checked_at, NOW);
        assert!(d.notice.is_none());

        let fresh = state(NOW - DAY + 1, None);
        let d = decide(&fresh, NOW, &current, true);
        assert!(!d.spawn);
        assert_eq!(d.state, fresh);

        assert!(decide(&state(NOW - DAY, None), NOW, &current, false).spawn);
        // visszaállított óra: a jövőbeli időbélyeg is lejár
        assert!(decide(&state(NOW + DAY, None), NOW, &current, true).spawn);
    }

    #[test]
    fn notices_a_newer_version_once_per_day_per_version() {
        let current = v("0.1.0");
        let s = state(NOW, Some("0.2.0"));
        let d = decide(&s, NOW, &current, true);
        assert_eq!(d.notice, Some(v("0.2.0")));
        assert_eq!(d.state.notified_at, Some(NOW));
        assert_eq!(d.state.notified_version.as_deref(), Some("0.2.0"));

        // ugyanarról a verzióról 24 órán belül nem, utána újra
        let again = decide(&d.state, NOW + 60, &current, true);
        assert!(again.notice.is_none());
        assert_eq!(again.state, d.state);
        assert!(decide(&d.state, NOW + DAY, &current, true).notice.is_some());

        // egy még újabb verzióról azonnal
        let newer = CheckState {
            latest: Some("0.3.0".into()),
            ..d.state.clone()
        };
        assert_eq!(
            decide(&newer, NOW + 60, &current, true).notice,
            Some(v("0.3.0"))
        );
    }

    #[test]
    fn no_notice_on_failure_or_when_not_newer() {
        let current = v("0.2.0");
        let s = state(NOW, Some("0.3.0"));
        let failed = decide(&s, NOW, &current, false);
        assert!(failed.notice.is_none());
        assert_eq!(failed.state, s);
        for latest in ["0.2.0", "0.1.9", "0.4.0-rc.1", "garbage"] {
            let d = decide(&state(NOW, Some(latest)), NOW, &current, true);
            assert!(d.notice.is_none(), "{latest}");
        }
    }

    #[test]
    fn auto_update_is_attempted_at_most_once_a_day() {
        let current = v("0.1.0");
        // még soha: próbálkozás, a két időbélyeg azonnal beírva
        let next = decide_auto(&CheckState::default(), NOW, &current).unwrap();
        assert_eq!((next.checked_at, next.auto_update_at), (NOW, Some(NOW)));
        // egy napon belül nem újra — akkor sem, ha közben kiderült, hogy van újabb
        let tried = CheckState {
            latest: Some("0.2.0".into()),
            ..next
        };
        assert_eq!(decide_auto(&tried, NOW + DAY - 1, &current), None);
        assert!(decide_auto(&tried, NOW + DAY, &current).is_some());
        // visszaállított óra: lejártnak számít
        assert!(decide_auto(&tried, NOW - DAY, &current).is_some());
    }

    #[test]
    fn auto_update_reuses_a_fresh_cache() {
        let current = v("0.1.0");
        // ma már ellenőrizte (pl. a wrap háttér-ellenőrzése), nincs újabb: nincs hálózat
        for latest in [None, Some("0.1.0"), Some("0.0.9"), Some("0.2.0-rc.1")] {
            assert_eq!(
                decide_auto(&state(NOW - 60, latest), NOW, &current),
                None,
                "{latest:?}"
            );
        }
        // ma ellenőrizte, és van újabb: most frissít (a friss gyorsítótár nem tartja vissza)
        let next = decide_auto(&state(NOW - 60, Some("0.2.0")), NOW, &current).unwrap();
        assert_eq!(next.auto_update_at, Some(NOW));
        assert_eq!(next.latest.as_deref(), Some("0.2.0"));
        // régi gyorsítótár: újra megnézzük
        assert!(decide_auto(&state(NOW - DAY, None), NOW, &current).is_some());
    }

    #[test]
    fn dev_builds_are_not_pulled_back_to_the_same_stable_base() {
        let dev = v("0.5.1-dev.23ed277");
        // ugyanaz az alapverzió (vagy régebbi): nincs értesítés, és az auto-update sem tér vissza a stabilra
        for latest in ["0.5.1", "0.5.0"] {
            assert!(
                decide(&state(NOW, Some(latest)), NOW, &dev, true)
                    .notice
                    .is_none()
            );
            assert_eq!(decide_auto(&state(NOW - 60, Some(latest)), NOW, &dev), None);
        }
        // újabb alapverzió: szól, és az auto-update (stabil csatorna) frissít
        let d = decide(&state(NOW, Some("0.5.2")), NOW, &dev, true);
        assert_eq!(d.notice, Some(v("0.5.2")));
        assert!(decide_auto(&state(NOW - 60, Some("0.5.2")), NOW, &dev).is_some());
    }
}
