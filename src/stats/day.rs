//! Naptári nap (UTC) külső crate nélkül: a napi összesítő kulcsa (`YYYY-MM-DD`) és a havi fájl neve (`YYYY-MM`).
//! Szabad szöveg nem lehet benne: csak egy szám (napok 1970-01-01 óta), a szöveg-alak ebből készül.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Egy UTC-nap: a napok száma 1970-01-01 óta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Day(i64);

impl Day {
    /// Unix-másodpercből.
    pub(crate) fn from_secs(secs: u64) -> Self {
        Self((secs / 86_400) as i64)
    }

    /// A mai (UTC) nap.
    pub(crate) fn today() -> Self {
        Self::from_secs(crate::update::check::now_secs())
    }

    /// Év, hónap, nap → nap (Hinnant-féle `days_from_civil`); érvénytelen dátumra `None`.
    pub(crate) fn from_ymd(year: i64, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        let y = if month <= 2 { year - 1 } else { year };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(month);
        let mp = if m > 2 { m - 3 } else { m + 9 };
        let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Some(Self(era * 146_097 + doe - 719_468))
    }

    /// Nap → (év, hónap, nap) (Hinnant-féle `civil_from_days`).
    pub(crate) fn ymd(self) -> (i64, u32, u32) {
        let z = self.0 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        (year, month, day)
    }

    /// `n` nappal korábban.
    pub(crate) fn minus(self, n: u32) -> Self {
        Self(self.0 - i64::from(n))
    }

    /// A hónap első napja.
    pub(crate) fn month_start(self) -> Self {
        let (y, m, _) = self.ymd();
        Self::from_ymd(y, m, 1).unwrap_or(self)
    }

    /// A következő hónap első napja.
    pub(crate) fn next_month_start(self) -> Self {
        let (y, m, _) = self.ymd();
        let (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        Self::from_ymd(y, m, 1).unwrap_or(self)
    }

    /// A havi fájl neve kiterjesztés nélkül: `YYYY-MM`.
    pub(crate) fn month_key(self) -> String {
        let (y, m, _) = self.ymd();
        format!("{y:04}-{m:02}")
    }

    /// `YYYY-MM-DD` → nap.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let mut parts = text.splitn(3, '-');
        let year = parts.next()?.parse().ok()?;
        let month = parts.next()?.parse().ok()?;
        let day = parts.next()?.parse().ok()?;
        Self::from_ymd(year, month, day)
    }

    /// `YYYY-MM` → a hónap első napja.
    pub(crate) fn parse_month(text: &str) -> Option<Self> {
        let (year, month) = text.split_once('-')?;
        Self::from_ymd(year.parse().ok()?, month.parse().ok()?, 1)
    }
}

impl std::fmt::Display for Day {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl Serialize for Day {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Day {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("invalid day"))
    }
}
