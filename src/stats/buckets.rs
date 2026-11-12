//! A sávok: futási idő, kimenet-méret (token ≈ karakter / 4) és a `locate`-rangsor találata. Pontos érték nem
//! tárolódik, csak a sáv sorszáma (a napi fájlban a sávonkénti darabszám-tömb indexe).

/// Futási idő sávja: <50 ms, 50–200 ms, 200–1000 ms, 1–5 s, 5 s+.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LatencyBucket {
    Under50Ms,
    To200Ms,
    To1S,
    To5S,
    Over5S,
}

impl LatencyBucket {
    /// A sávok száma (a napi fájl tömbjének hossza).
    pub(crate) const COUNT: usize = 5;
    /// A sávok címkéi a riportban, sorrendben.
    pub(crate) const LABELS: [&str; Self::COUNT] = ["<50ms", "50-200ms", "0.2-1s", "1-5s", "5s+"];

    /// Ezredmásodpercből; a határ a felső sávba tartozik (50 ms → 50–200 ms).
    pub(crate) fn from_millis(ms: u128) -> Self {
        match ms {
            0..50 => Self::Under50Ms,
            50..200 => Self::To200Ms,
            200..1000 => Self::To1S,
            1000..5000 => Self::To5S,
            _ => Self::Over5S,
        }
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// Kimenet-méret sávja tokenben (karakter / 4): <500, 500–2k, 2k–8k, 8k+.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SizeBucket {
    Under500,
    To2K,
    To8K,
    Over8K,
}

impl SizeBucket {
    pub(crate) const COUNT: usize = 4;
    pub(crate) const LABELS: [&str; Self::COUNT] = ["<500", "500-2k", "2k-8k", "8k+"];

    /// A kimenet karakterszámából (token ≈ karakter / 4); a határ a felső sávba tartozik.
    pub(crate) fn from_chars(chars: usize) -> Self {
        match chars / 4 {
            0..500 => Self::Under500,
            500..2000 => Self::To2K,
            2000..8000 => Self::To8K,
            _ => Self::Over8K,
        }
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }

    pub(crate) fn label(self) -> &'static str {
        Self::LABELS[self.index()]
    }
}

/// A `locate` után megnyitott fájl helye a rangsorban: 1., 2–5., 6+., vagy nem volt a listán.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RankBucket {
    First,
    Top5,
    Beyond,
    NotListed,
}

impl RankBucket {
    pub(crate) const COUNT: usize = 4;

    /// 1-től számozott helyezésből.
    pub(crate) fn from_rank(rank: usize) -> Self {
        match rank {
            0 | 1 => Self::First,
            2..=5 => Self::Top5,
            _ => Self::Beyond,
        }
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// A célfájl (bemenet) méretének sávja bájtban: <10k, 10–50k, 50–200k, 200k+ (k = 1000 bájt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputBucket {
    Under10K,
    To50K,
    To200K,
    Over200K,
}

impl InputBucket {
    pub(crate) const COUNT: usize = 4;
    pub(crate) const LABELS: [&str; Self::COUNT] = ["<10k", "10-50k", "50-200k", "200k+"];

    /// A fájl bájtméretéből; a határ a felső sávba tartozik (10 000 bájt → 10–50k).
    pub(crate) fn from_bytes(bytes: u64) -> Self {
        match bytes {
            0..10_000 => Self::Under10K,
            10_000..50_000 => Self::To50K,
            50_000..200_000 => Self::To200K,
            _ => Self::Over200K,
        }
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }

    pub(crate) fn label(self) -> &'static str {
        Self::LABELS[self.index()]
    }
}
