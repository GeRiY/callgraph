//! Szemantikus verzió (`MAJOR.MINOR.PATCH[-pre][+build]`) összehasonlításhoz — a release tag (`v0.2.0`) és a
//! `CARGO_PKG_VERSION`. A build-metaadat nem számít; az előzetes (pre-release) verzió régebbi a kiadottnál.

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

    /// Ennek a binárisnak a verziója (`Cargo.toml`).
    pub(crate) fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("a Cargo.toml verziója semver")
    }

    /// Előzetes kiadás-e (`1.0.0-rc.1`) — a frissítés ezeket kihagyja.
    pub(crate) fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
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
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
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
}
