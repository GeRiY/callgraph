//! A szerepek (rétegek) táblája: név, fajta, a hívási irányban elfoglalt hely (rang) és a hozzá tartozó minta.
//! Új szerep: egy sor a [`ROLES`]-ban; a szabályok (`rules.rs`) név szerint hivatkoznak rá.

use std::fmt;

/// A szerep fajtája: a profilban és a hívásirány-ellenőrzésben másként számít.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Alkalmazás-réteg (controller, service, repository…): a hívásirányba beszámít.
    Layer,
    /// Tervezési minta saját mappával / végződéssel (factory, strategy…): a réteg mellé minta-címke.
    Pattern,
    /// DDD / hexagonális kontextus-mappa (Domain, Application…): csak ha a mélyebb név semmit sem mond.
    Context,
    /// Kiszolgáló fájlok (teszt, migráció, konfiguráció, provider): a hívásirányba nem számít.
    Support,
}

/// Egy szerep adatai.
#[derive(Debug)]
pub(crate) struct RoleInfo {
    pub name: &'static str,
    pub kind: Kind,
    /// A várt hívási irányban elfoglalt hely: a kisebb hívja a nagyobbat (route → controller → service → model).
    pub rank: u8,
    /// A szerephez tartozó tervezési minta neve (a profil `patterns:` sorához).
    pub pattern: Option<&'static str>,
}

const fn role(name: &'static str, kind: Kind, rank: u8, pattern: Option<&'static str>) -> RoleInfo {
    RoleInfo {
        name,
        kind,
        rank,
        pattern,
    }
}

/// Az ismert szerepek, a kiírás sorrendjében (a rang szerint rendezve íródnak ki).
pub(crate) static ROLES: &[RoleInfo] = &[
    role("route", Kind::Layer, 0, None),
    role("middleware", Kind::Layer, 1, None),
    role("controller", Kind::Layer, 2, None),
    role("handler", Kind::Layer, 2, Some("Command / Handler")),
    role("command", Kind::Layer, 2, Some("Command / Handler")),
    role("page", Kind::Layer, 2, None),
    role("listener", Kind::Layer, 2, Some("Observer / Listener")),
    role("observer", Kind::Layer, 2, Some("Observer / Listener")),
    role("job", Kind::Layer, 2, None),
    role("view", Kind::Layer, 3, None),
    role("component", Kind::Layer, 3, None),
    role("request", Kind::Layer, 3, None),
    role("policy", Kind::Layer, 4, None),
    role("viewmodel", Kind::Layer, 4, None),
    role("composable", Kind::Layer, 4, None),
    role("store", Kind::Layer, 5, None),
    role("event", Kind::Layer, 5, None),
    role("service", Kind::Layer, 6, None),
    role("api", Kind::Layer, 7, None),
    role("repository", Kind::Layer, 8, Some("Repository")),
    role("dto", Kind::Layer, 8, Some("DTO")),
    role("model", Kind::Layer, 9, None),
    role("factory", Kind::Pattern, 7, Some("Factory")),
    role("builder", Kind::Pattern, 7, Some("Builder")),
    role("strategy", Kind::Pattern, 7, Some("Strategy")),
    role("adapter", Kind::Pattern, 7, Some("Adapter")),
    role("decorator", Kind::Pattern, 7, Some("Decorator")),
    role("presentation", Kind::Context, 2, None),
    role("application", Kind::Context, 6, None),
    role("infrastructure", Kind::Context, 7, None),
    role("port", Kind::Context, 8, None),
    role("domain", Kind::Context, 9, None),
    role("provider", Kind::Support, 1, None),
    role("migration", Kind::Support, 10, None),
    role("config", Kind::Support, 10, None),
    role("test", Kind::Support, 11, None),
];

/// Többes számú / rokon alakok a felhasználói bemenethez (`--layer`, `.callgraph-layers`).
const ALIASES: &[(&str, &str)] = &[
    ("entity", "model"),
    ("entities", "model"),
    ("resource", "dto"),
    ("resources", "dto"),
    ("hook", "composable"),
    ("hooks", "composable"),
    ("repositories", "repository"),
    ("repo", "repository"),
    ("policies", "policy"),
    ("factories", "factory"),
    ("strategies", "strategy"),
    ("middlewares", "middleware"),
];

/// Egy szerep (a [`ROLES`] indexe): olcsón másolható, összehasonlítható.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Role(u8);

impl Role {
    /// Pontos név a táblából (a szabályok így hivatkoznak rá).
    pub(crate) fn named(name: &str) -> Option<Self> {
        ROLES
            .iter()
            .position(|info| info.name == name)
            .and_then(|at| u8::try_from(at).ok())
            .map(Self)
    }

    /// Felhasználói bemenet: kisbetűsítve, a rokon alakok és a többes szám (`services`) is.
    pub(crate) fn parse(input: &str) -> Option<Self> {
        let name = input.trim().to_ascii_lowercase();
        if let Some(role) = Self::named(&name) {
            return Some(role);
        }
        if let Some((_, target)) = ALIASES.iter().find(|(alias, _)| *alias == name) {
            return Self::named(target);
        }
        name.strip_suffix('s').and_then(Self::named)
    }

    pub(crate) fn info(self) -> &'static RoleInfo {
        &ROLES[usize::from(self.0)]
    }

    pub(crate) fn name(self) -> &'static str {
        self.info().name
    }

    pub(crate) fn kind(self) -> Kind {
        self.info().kind
    }

    pub(crate) fn rank(self) -> u8 {
        self.info().rank
    }

    /// Beszámít-e a rétegek közti hívásirányba (alkalmazás-réteg vagy kontextus).
    pub(crate) fn in_flow(self) -> bool {
        matches!(self.kind(), Kind::Layer | Kind::Context)
    }

    /// Az összes szerep neve vesszővel (a hibaüzenethez).
    pub(crate) fn known_names() -> String {
        ROLES
            .iter()
            .map(|info| info.name)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// A kiírás sorrendje: rang, a kiszolgáló fájlok a végén, azonos rangon a tábla sorrendje.
    pub(crate) fn display_order(self) -> (bool, u8, u8) {
        (self.kind() == Kind::Support, self.rank(), self.0)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A besorolás bizonyossága.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    /// A kiírt név a kimenet nyelvén.
    pub(crate) fn label(self, lang: crate::i18n::Lang) -> &'static str {
        match self {
            Self::High => lang.pick("high", "magas"),
            Self::Medium => lang.pick("medium", "közepes"),
            Self::Low => lang.pick("low", "alacsony"),
        }
    }
}
