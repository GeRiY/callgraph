//! Szemantikus verzió (`MAJOR.MINOR.PATCH[-pre][+build]`) összehasonlításhoz — a release tag (`v0.2.0`) és a
//! `CARGO_PKG_VERSION`. A build-metaadat nem számít; az előzetes (pre-release) verzió régebbi a kiadottnál.
//! A develop-ág CI-buildje (dev-csatorna) verziója `X.Y.Z-dev.<sha7>`: a build-időben megadott
//! `CALLGRAPH_BUILD_VERSION` (lásd `build_version`); helyi és tag-buildnél a `Cargo.toml` verziója.

use std::cmp::Ordering;
use std::fmt;

/// Egy értelmezett verzió; az összehasonlítás a semver 2.0 szerint (a pre-release azonosítók szövegként / számként).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Version {
    core: [u64; 3],
    pre: Vec<String>,
}

impl Version {
    /// `0.1.0`, `v0.2.0`, `1.2.3-rc.1+abc` → verzió; hibás formánál `None`.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        let text = text.split_once('+').map_or(text, |(v, _)| v);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, pre.split('.').map(str::to_owned).collect()),
            None => (text, Vec::new()),
        };
        let mut parts = core.split('.');
        let mut numbers = [0u64; 3];
        for n in &mut numbers {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *n = part.parse().ok()?;
        }
        if parts.next().is_some() || pre.iter().any(String::is_empty) {
            return None;
        }
        Some(Self { core: numbers, pre })
    }

    /// Ennek a binárisnak a verziója (`build_version`: dev-buildnél `X.Y.Z-dev.<sha7>`, különben a `Cargo.toml`-é).
    pub(crate) fn current() -> Self {
        Self::parse(build_version()).expect("a Cargo.toml verziója semver")
    }

    /// Előzetes kiadás-e (`1.0.0-rc.1`) — a frissítés ezeket kihagyja.
    pub(crate) fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    /// A develop-ág dev-buildje-e (`X.Y.Z-dev.<sha7>`).
    pub(crate) fn is_dev(&self) -> bool {
        self.pre.len() == 2 && self.pre[0] == "dev"
    }

    /// Egy stabil kiadás (`self`) frissítés-e a `current`-hez a háttér-értesítés és az MCP `--auto-update`
    /// számára: dev-buildről csak egy újabb alapverzió (a `0.5.1-dev.x` mellett a `0.5.1` nem, a `0.5.2` igen) —
    /// így a dev-csatornát a felhasználó csak kifejezett `update`-tel hagyja el; különben a semver szerint újabb.
    pub(crate) fn supersedes(&self, current: &Version) -> bool {
        if self.is_prerelease() {
            return false;
        }
        if current.is_dev() {
            return self.core > current.core;
        }
        self > current
    }

    /// A dev-csatorna döntése: a `self` dev-build újabb-e a telepített `current`-nél. Dev-buildről: ha más a
    /// commit (vagy az alapverzió); stabil (vagy rc) verzióról: ha az alapverziója legfeljebb a dev-build alapja.
    pub(crate) fn dev_is_newer(&self, current: &Version) -> bool {
        if current.is_dev() {
            return self != current;
        }
        current.core <= self.core
    }
}

/// A bináris verzió-szövege (`--version`, MCP `serverInfo`): a build-időben megadott `CALLGRAPH_BUILD_VERSION`
/// (a CI csak a develop-ág buildjeinél állítja `X.Y.Z-dev.<sha7>`-re), ha érvényes verzió; különben a
/// `Cargo.toml` verziója.
pub(crate) fn build_version() -> &'static str {
    match option_env!("CALLGRAPH_BUILD_VERSION") {
        Some(v) if Version::parse(v).is_some() => v,
        _ => env!("CARGO_PKG_VERSION"),
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => cmp_pre(&self.pre, &other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Pre-release azonosítók: a numerikus számként és mindig kisebb a szövegesnél; a rövidebb lista a kisebb.
fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.core;
        write!(f, "{major}.{minor}.{patch}")?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn parses_tags_and_rejects_garbage() {
        assert_eq!(v("v0.1.0"), v("0.1.0"));
        assert_eq!(v("1.2.3+build.5").to_string(), "1.2.3");
        assert_eq!(v("1.2.3-rc.1").to_string(), "1.2.3-rc.1");
        assert!(v("1.0.0-beta").is_prerelease());
        for bad in [
            "", "1", "1.2", "1.2.3.4", "1.x.3", "v", "1.2.3-", "-1.2.3", "1..3",
        ] {
            assert!(Version::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(Version::current().to_string(), build_version());
        assert!(build_version().starts_with(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn compares_numerically_and_by_semver_rules() {
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert!(v("0.1.1") > v("0.1.0"));
        assert_eq!(v("0.1.0").cmp(&v("v0.1.0+x")), Ordering::Equal);
        assert!(v("1.0.0") > v("1.0.0-rc.1"));
        assert!(v("1.0.0-rc.2") > v("1.0.0-rc.1"));
        assert!(v("1.0.0-rc.10") > v("1.0.0-rc.9"));
        assert!(v("1.0.0-beta") > v("1.0.0-alpha.1"));
        assert!(v("1.0.0-alpha.1") > v("1.0.0-alpha"));
        assert!(v("1.0.0-alpha.beta") > v("1.0.0-alpha.1"));
    }

    #[test]
    fn dev_builds_follow_the_channel_rules() {
        let dev = v("0.5.1-dev.23ed277");
        assert!(dev.is_dev() && dev.is_prerelease());
        assert!(!v("0.5.1").is_dev() && !v("0.5.1-rc.1").is_dev() && !v("0.5.1-dev").is_dev());
        assert_eq!(dev.to_string(), "0.5.1-dev.23ed277");
        assert!(dev < v("0.5.1") && dev > v("0.5.0"));

        // dev-csatorna: más commit = újabb; stabil telepítéshez képest az alapverzió dönt
        assert!(v("0.5.1-dev.abc1234").dev_is_newer(&dev));
        assert!(!dev.dev_is_newer(&dev));
        assert!(dev.dev_is_newer(&v("0.5.1")));
        assert!(dev.dev_is_newer(&v("0.5.0")));
        assert!(dev.dev_is_newer(&v("0.5.1-rc.1")));
        assert!(!dev.dev_is_newer(&v("0.5.2")));

        // stabil csatorna (értesítés, auto-update): dev-buildről csak újabb alapverzió
        assert!(!v("0.5.1").supersedes(&dev));
        assert!(!v("0.5.0").supersedes(&dev));
        assert!(v("0.5.2").supersedes(&dev));
        assert!(v("0.5.1").supersedes(&v("0.5.0")));
        assert!(!v("0.5.1").supersedes(&v("0.5.1")));
        assert!(!v("0.6.0-rc.1").supersedes(&v("0.5.1")));
    }
}
