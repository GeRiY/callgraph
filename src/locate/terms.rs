//! A keresett kifejezések és az azonosítók szavakra bontása: kisbetűsítés, camelCase / PascalCase / snake_case /
//! kebab-case / pont mentén bontás, egyes / többes szám tűrés (`refund` ~ `refunds`, `status` ~ `statuses`,
//! `category` ~ `categories`). Két különleges alak: a szóközt tartalmazó argumentum egyetlen kifejezés (`Phrase`:
//! felirat / fordítási szöveg, egészében a string-literálokra illesztve), a `route::név` pedig egy útvonal neve
//! (`Route`: a `route('név')` / `to_route` / `redirect()->route` hívások stringjei és a `->name('név')` definíció).

/// A kifejezés fajtája: név / fogalom, több szavas kifejezés, vagy útvonalnév.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TermKind {
    /// Név vagy fogalom (függvény, osztály, fájl, oszlop): pontos, név-részlet, string és hívás egyezés.
    Name,
    /// Szóközt tartalmazó kifejezés (`"Szervező további eseményei"`): csak string-literál, egészében, kis- /
    /// nagybetűtől és a szóközök számától függetlenül; nem bontva, név-részletként nem illeszt.
    Phrase,
    /// `route::név`: a név kisbetűvel (`events.show`).
    Route(String),
}

/// Egy keresett kifejezés, ahogy az agent átadta, és az illesztéshez előkészített alakjai.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Term {
    /// Az átadott alak (a kimenetben így jelenik meg).
    pub raw: String,
    /// Kisbetűs alak (pontos egyezéshez); a több szavas kifejezésnél egy szóközzel.
    pub lower: String,
    /// A kifejezés szavai kisbetűvel (név-részlet egyezéshez; a `Phrase` és a `Route` kifejezésnél üres).
    pub words: Vec<String>,
    /// `Típus::metódus` / `Típus.metódus` / `Típus->metódus` alak: (a típus rövid neve, a név), kisbetűvel.
    pub member: Option<(String, String)>,
    /// A kifejezés fajtája.
    pub kind: TermKind,
}

/// Az útvonalnév-kifejezés előtagja.
const ROUTE_PREFIX: &str = "route::";

impl Term {
    /// Egy kifejezés előkészítése; a körülötte álló idézőjelek / backtickek lekerülnek. Üres kifejezés: `None`.
    /// Szóközzel: `Phrase`; `route::név`: `Route`; különben `Name`.
    pub(crate) fn new(raw: &str) -> Option<Self> {
        let raw = raw.trim().trim_matches(['"', '\'', '`']).trim();
        if raw.is_empty() {
            return None;
        }
        if raw.contains(char::is_whitespace) {
            let raw = raw.split_whitespace().collect::<Vec<_>>().join(" ");
            return Some(Self {
                lower: raw.to_lowercase(),
                raw,
                words: Vec::new(),
                member: None,
                kind: TermKind::Phrase,
            });
        }
        let lower = raw.to_lowercase();
        if let Some(route) = lower.strip_prefix(ROUTE_PREFIX).filter(|r| !r.is_empty()) {
            let route = route.to_owned();
            return Some(Self {
                raw: raw.to_owned(),
                lower,
                words: Vec::new(),
                member: None,
                kind: TermKind::Route(route),
            });
        }
        let member = match split_member(&lower) {
            (Some(owner), name) if !name.is_empty() => {
                let owner = short_name(owner);
                (!owner.is_empty()).then(|| (owner.to_owned(), name.to_owned()))
            }
            _ => None,
        };
        Some(Self {
            raw: raw.to_owned(),
            words: words(raw),
            lower,
            member,
            kind: TermKind::Name,
        })
    }

    /// A parancssori / MCP-bemenet kifejezései, kis-/nagybetűtől függetlenül egyszer. A szóközt tartalmazó elem
    /// egyetlen kifejezés (`Phrase`); a szóköz nélküli vessző mentén bontva (egy agent néha egy stringben küld
    /// többet: `refund,payment_status`).
    pub(crate) fn parse_all<S: AsRef<str>>(inputs: &[S]) -> Vec<Self> {
        let mut out: Vec<Self> = Vec::new();
        for input in inputs {
            let input = input.as_ref().trim();
            let parts: Vec<&str> = if input.contains(char::is_whitespace) {
                vec![input]
            } else {
                input.split(',').collect()
            };
            for part in parts {
                let Some(term) = Self::new(part) else {
                    continue;
                };
                if !out.iter().any(|t| t.lower == term.lower) {
                    out.push(term);
                }
            }
        }
        out
    }

    /// Név / fogalom kifejezés-e (a `Phrase` és a `Route` csak a saját egyezéseit adja).
    pub(crate) fn is_plain(&self) -> bool {
        self.kind == TermKind::Name
    }

    /// Pontosan ez a név-e (kis-/nagybetűtől függetlenül; névtérrel adott névnél a rövid név is számít).
    pub(crate) fn is_name(&self, name: &str) -> bool {
        let name = name.to_lowercase();
        name == self.lower || short_name(&name) == self.lower
    }

    /// `Típus::név` alakú kifejezésnél: a típus és a név is egyezik-e (kisbetűs bemenettel).
    pub(crate) fn is_member(&self, owner: Option<&str>, name: &str) -> bool {
        match (&self.member, owner) {
            (Some((want_owner, want_name)), Some(owner)) => {
                want_name == name && short_name(owner) == want_owner
            }
            _ => false,
        }
    }

    /// Név-részlet egyezés: a kifejezés minden szava megvan az azonosító szavai között (egyes / többes szám tűréssel).
    /// Túl rövid (3 betűnél rövidebb) kifejezés nem illeszt név-részletre.
    pub(crate) fn matches_words(&self, ident_words: &[String]) -> bool {
        self.lower.chars().count() >= 3
            && !self.words.is_empty()
            && self
                .words
                .iter()
                .all(|w| ident_words.iter().any(|x| same_word(x, w)))
    }
}

/// Egy azonosító (vagy kifejezés) szavai kisbetűvel. Határ: minden nem betű / számjegy karakter (`_`, `-`, `.`,
/// `\`, `/`, `:`, `$`, szóköz), kisbetű / számjegy → nagybetű (`processRefund`), és a nagybetűs rövidítés vége
/// (`HTTPServer` → `http`, `server`).
pub(crate) fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in text.split(|c: char| !c.is_alphanumeric()) {
        let chars: Vec<char> = part.chars().collect();
        let mut current = String::new();
        for (i, &c) in chars.iter().enumerate() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1).copied();
            let boundary = c.is_uppercase()
                && prev.is_some_and(|p| {
                    p.is_lowercase()
                        || p.is_numeric()
                        || (p.is_uppercase() && next.is_some_and(char::is_lowercase))
                });
            if boundary && !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            current.extend(c.to_lowercase());
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}

/// Két kisbetűs szó azonos-e, egyes / többes szám tűréssel (`+s`, `+es`, `y` → `ies`).
pub(crate) fn same_word(a: &str, b: &str) -> bool {
    a == b || plural_of(a, b) || plural_of(b, a)
}

/// `plural` a `singular` többes száma-e. Egybetűs szóra nem (az `a` ~ `as` zaj volna).
fn plural_of(plural: &str, singular: &str) -> bool {
    if singular.chars().count() < 2 {
        return false;
    }
    match plural.strip_prefix(singular) {
        Some("s" | "es") => true,
        _ => singular
            .strip_suffix('y')
            .is_some_and(|stem| plural.strip_prefix(stem) == Some("ies")),
    }
}

/// Egy tag-hivatkozás szétválasztása a legutolsó `::`, `->` vagy `.` mentén: (tulajdonos, név). Elválasztó nélkül
/// a tulajdonos `None`. A `?::név` (ismeretlen fogadó) tulajdonosa `?`.
pub(crate) fn split_member(text: &str) -> (Option<&str>, &str) {
    let at = ["::", "->", "."]
        .iter()
        .filter_map(|sep| text.rfind(sep).map(|at| (at, sep.len())))
        .max_by_key(|(at, _)| *at);
    match at {
        Some((at, len)) => (Some(&text[..at]), &text[at + len..]),
        None => (None, text),
    }
}

/// Egy típus- vagy fogadó-kifejezés rövid neve: a névtér (`\`), a lánc (`$this->repo` → `repo`) és a `$` nélkül.
pub(crate) fn short_name(text: &str) -> &str {
    let text = text.rsplit('\\').next().unwrap_or(text);
    let (_, last) = split_member(text);
    last.trim_start_matches('$')
}
