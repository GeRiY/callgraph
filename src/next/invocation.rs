//! A futtatott parancs a nyers argumentumaiból (`wrap …` a burokból, `show …` az MCP-ből és a tárolt
//! javaslatokból, `locate …`, `profile …`): csak annyi, amennyi a szabályoknak kell. Minden más parancs: `None`
//! (nincs javaslat).

use crate::stats::CommandKind;

/// A gépi formátumok: ezekhez soha nincs javaslat-sor.
const MACHINE_FORMATS: [&str; 4] = ["json", "jsonl", "mermaid", "dot"];

/// A szabályok bemenete: melyik parancs, milyen kapcsolókkal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Invocation {
    Locate { project: Option<String> },
    Profile { project: Option<String>, full: bool },
    Show(ShowCall),
}

/// Egy `show` / `wrap` hívás.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ShowCall {
    pub project: Option<String>,
    pub targets: Vec<String>,
    pub grep: Option<String>,
    pub format: Option<String>,
    pub depth: Option<String>,
    pub layer: Option<String>,
    pub body: bool,
    pub by_layer: bool,
    /// Egyéb szűkítő / módosító kapcsoló (`--lines`, `--strings`, `--const`, `--category`, `--notes`, `--layers`,
    /// `--full`, `--guess`, `--doc`, `--raw`): ilyenkor a szabályok nem javasolnak.
    pub other: bool,
}

impl ShowCall {
    /// Gépi formátum-e (json, jsonl, mermaid, dot).
    pub(crate) fn machine(&self) -> bool {
        self.format
            .as_deref()
            .is_some_and(|f| MACHINE_FORMATS.contains(&f))
    }

    /// Az alap szöveges nézet-e (outline vagy formátum nélkül).
    pub(crate) fn plain_format(&self) -> bool {
        matches!(self.format.as_deref(), None | Some("outline"))
    }
}

impl Invocation {
    /// A bináris argumentumaiból (a program neve nélkül); `None`, ha a parancshoz nincs szabály.
    pub(crate) fn parse(args: &[String]) -> Option<Self> {
        let (head, rest) = args.split_first()?;
        match head.as_str() {
            "locate" => Some(Self::Locate {
                project: value_of(rest, "--project"),
            }),
            "profile" => Some(Self::Profile {
                project: value_of(rest, "--project"),
                full: rest.iter().any(|a| a == "--full"),
            }),
            "show" | "wrap" => Some(Self::Show(parse_show(rest))),
            _ => None,
        }
    }

    /// A `--project` értéke (alias vagy mappa), ha volt.
    pub(crate) fn project(&self) -> Option<&str> {
        match self {
            Self::Locate { project } | Self::Profile { project, .. } => project.as_deref(),
            Self::Show(call) => call.project.as_deref(),
        }
    }

    /// A statisztika parancs-fajtája.
    pub(crate) fn command_kind(&self) -> CommandKind {
        match self {
            Self::Locate { .. } => CommandKind::Locate,
            Self::Profile { .. } => CommandKind::Profile,
            Self::Show(_) => CommandKind::Show,
        }
    }

    /// A hívás tárgyai (fájlnevek, metódusnevek, a hívó-minta neve): a „visszalépés” jel összevetéséhez.
    pub(crate) fn subjects(&self) -> Vec<String> {
        let Self::Show(call) = self else {
            return Vec::new();
        };
        let mut subjects = Vec::new();
        for target in &call.targets {
            let mut parts = target.split("::");
            if let Some(file) = parts.next() {
                let name = file.trim_end_matches('/').rsplit('/').next().unwrap_or(file);
                subjects.push(name.to_owned());
            }
            subjects.extend(parts.map(str::to_owned));
        }
        if let Some(name) = call.grep.as_deref().and_then(pattern_name) {
            subjects.push(name);
        }
        subjects.retain(|s| !s.is_empty());
        subjects
    }
}

/// A név szerinti hívó-minta neve: `(^|::|\.|->)refund$` → `refund` (a záró azonosító-karakterek).
pub(crate) fn pattern_name(pattern: &str) -> Option<String> {
    let body = pattern.strip_suffix('$').unwrap_or(pattern);
    let start = body
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map(|(at, _)| at)?;
    Some(body[start..].to_owned())
}

fn value_of(args: &[String], flag: &str) -> Option<String> {
    let at = args.iter().position(|a| a == flag)?;
    args.get(at + 1).cloned()
}

fn parse_show(args: &[String]) -> ShowCall {
    let mut call = ShowCall::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned();
        match a.as_str() {
            "--project" => call.project = value(),
            "--label" => {
                value();
            }
            "--grep" => call.grep = value(),
            "--format" => call.format = value(),
            "--depth" => call.depth = value(),
            "--layer" => call.layer = value(),
            "--body" => call.body = true,
            "--by-layer" => call.by_layer = true,
            "--lines" | "--strings" | "--const" | "--category" | "--notes" => {
                value();
                call.other = true;
            }
            "--layers" | "--full" | "--guess" | "--doc" | "--raw" => call.other = true,
            // KULCS=érték előtag a burokban (pl. CALLGRAPH_OUT_LIMIT=…): nem cél
            other if other.contains('=') && !other.contains('/') && !other.starts_with('-') => {}
            other if other.starts_with("--") => call.other = true,
            other => call.targets.push(other.to_owned()),
        }
    }
    call
}
