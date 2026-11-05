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
        .filter(|latest| succeeded && latest > current && !latest.is_prerelease())
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
}
