//! `callgraph wrap …` — a `callgraph.sh` burok logikája: projekt-alias feloldása, `show` futtatása,
//! hibánál error-id-s naplózás, utána az automatikus frissítés-ellenőrzés (`update::check`: naponta legfeljebb
//! egy háttérfolyamat, újabb verziónál egysoros értesítés a kimenet végén). A burok csak a súgót írja ki és ide továbbít.

use std::io::Write;

use crate::cli::{CliArgs, Command, build_command};
use crate::error::Result;
use crate::skill::{ProjectRegistry, SkillFailure, SkillHome, finish};
use crate::update::check::{UpdateCheck, now_secs};

const SOURCE: &str = "callgraph.sh";
fn usage() -> &'static str {
    crate::t!(
        "Usage: bash ~/.claude/skills/callgraph/callgraph.sh <file|dir>[::method] [...] [--project alias] [--format outline|api|stats|mermaid|dot|json]",
        "Használat: bash ~/.claude/skills/callgraph/callgraph.sh <fájl|mappa>[::metódusnév] [...] [--project alias] [--format outline|api|stats|mermaid|dot|json]"
    )
}

/// A `show` „nincs meg” hibáinak eleje mindkét nyelven (a query-motor szövegei): ezek `not_found` kontextust kapnak.
const NOT_FOUND_PREFIXES: [&str; 4] = [
    "Not found",
    "No such method",
    "Nem található",
    "Nincs ilyen metódus",
];

/// A `wrap` parancs; a frissítés-ellenőrzés DI-vel cserélhető.
pub struct WrapCommand {
    args: Vec<String>,
    home: SkillHome,
    update_check: UpdateCheck,
}

/// A burok-argumentumok szétválogatva: alias, a `show`-nak továbbadott kapcsolók és a célok.
#[derive(Debug, Default, PartialEq)]
struct Parsed {
    project: Option<String>,
    depth: String,
    forward: Vec<String>,
    targets: Vec<String>,
}

fn parse(args: &[String]) -> Parsed {
    let mut p = Parsed {
        depth: "1".into(),
        ..Parsed::default()
    };
    let mut it = args.iter().peekable();
    // KULCS=érték előtag: a CALLGRAPH_OUT_LIMIT-et a show argumentumként is érti, a többi hatástalan
    while let Some(a) = it.next_if(|a| is_env_assignment(a)) {
        if a.starts_with("CALLGRAPH_OUT_LIMIT=") {
            p.forward.push(a.clone());
        }
    }
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().unwrap_or_default();
        match a.as_str() {
            "--project" => p.project = Some(value()),
            "--depth" => {
                p.depth = value();
                p.forward.extend(["--depth".into(), p.depth.clone()]);
            }
            "--format" | "--grep" | "--category" | "--lines" | "--strings" | "--const"
            | "--notes" | "--layer" => {
                let v = value();
                p.forward.extend([a.clone(), v]);
            }
            "--body" | "--doc" | "--raw" | "--full" | "--guess" | "--layers" | "--by-layer" => {
                p.forward.push(a.clone())
            }
            _ => p.targets.push(a.clone()),
        }
    }
    p
}

fn is_env_assignment(a: &str) -> bool {
    a.split_once('=').is_some_and(|(k, _)| {
        k.chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase() || c == '_')
            && k.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    })
}

impl WrapCommand {
    pub fn new(args: Vec<String>, home: SkillHome) -> Self {
        let update_check = UpdateCheck::from_env(&home);
        Self {
            args,
            home,
            update_check,
        }
    }

    fn exec(&self, out: &mut dyn Write) -> std::result::Result<(), SkillFailure> {
        let p = parse(&self.args);
        let depth_ok = p.depth.starts_with(|c: char| ('1'..='9').contains(&c))
            && p.depth.chars().all(|c| c.is_ascii_digit());
        if !depth_ok {
            return Err(SkillFailure::usage(crate::tf!(
                "--depth must be a positive integer (got: {}).",
                "A --depth pozitív egész (kaptam: {}).",
                p.depth
            )));
        }
        if p.targets.is_empty() {
            return Err(SkillFailure::usage(usage()));
        }
        let root = match &p.project {
            None => std::env::current_dir()?.display().to_string(),
            Some(alias) => self.resolve(alias)?,
        };
        let label = p.project.clone().unwrap_or_else(|| root.clone());
        let mut show = vec![
            "show".into(),
            "--project".into(),
            root,
            "--label".into(),
            label,
        ];
        show.extend(p.forward);
        show.extend(p.targets);
        // a show hibaszövege (stderr) lesz az üzenet; sikeres futásnál a stderr-t a régi burok is eldobta
        let mut captured = Vec::new();
        let code = match CliArgs::parse(show).and_then(build_command) {
            Ok(command) => command.run(out, &mut captured).or_else(|e| {
                writeln!(captured, "{e}")?;
                Ok::<i32, crate::error::CallgraphError>(1)
            })?,
            Err(e) => {
                writeln!(captured, "{e}")?;
                2
            }
        };
        if code == 0 {
            return Ok(());
        }
        let message = String::from_utf8_lossy(&captured)
            .trim_end_matches('\n')
            .to_owned();
        if NOT_FOUND_PREFIXES.iter().any(|p| message.starts_with(p)) {
            Err(SkillFailure::not_found(message))
        } else {
            Err(SkillFailure::usage(message))
        }
    }

    fn resolve(&self, alias: &str) -> std::result::Result<String, SkillFailure> {
        let file = self.home.registry_file();
        if !file.is_file() {
            return Err(SkillFailure::usage(crate::tf!(
                "No projects.json — register a project first: bash ~/.claude/skills/callgraph/projects.sh add {alias} <absolute_path>",
                "Nincs projects.json — regisztrálj előbb: bash ~/.claude/skills/callgraph/projects.sh add {alias} <abszolút_útvonal>"
            )));
        }
        ProjectRegistry::load(file)?.root(alias).ok_or_else(|| {
            SkillFailure::usage(crate::tf!(
                "Unknown project alias: {alias} (see: bash ~/.claude/skills/callgraph/projects.sh list)",
                "Ismeretlen projekt-alias: {alias} (lásd: bash ~/.claude/skills/callgraph/projects.sh list)"
            ))
        })
    }
}

impl Command for WrapCommand {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let result = self.exec(out);
        self.update_check
            .after_command(now_secs(), result.is_ok(), out);
        finish(&self.home, SOURCE, result, err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn splits_project_forward_and_targets() {
        let p = parse(&s(&[
            "CALLGRAPH_OUT_LIMIT=5",
            "FOO=1",
            "--project",
            "sp",
            "a.php::m",
            "--format",
            "flow",
            "--depth",
            "2",
            "--guess",
        ]));
        assert_eq!(p.project.as_deref(), Some("sp"));
        assert_eq!(p.depth, "2");
        assert_eq!(
            p.forward,
            s(&[
                "CALLGRAPH_OUT_LIMIT=5",
                "--format",
                "flow",
                "--depth",
                "2",
                "--guess"
            ])
        );
        assert_eq!(p.targets, s(&["a.php::m"]));
    }

    #[test]
    fn forwards_the_layer_flags() {
        let p = parse(&s(&["app", "--layer", "service", "--layers", "--by-layer"]));
        assert_eq!(p.forward, s(&["--layer", "service", "--layers", "--by-layer"]));
        assert_eq!(p.targets, s(&["app"]));
    }

    #[test]
    fn env_assignment_only_at_front_and_uppercase() {
        assert!(is_env_assignment("CALLGRAPH_OUT_LIMIT=50"));
        assert!(!is_env_assignment("a=b"));
        assert_eq!(
            parse(&s(&["x.php", "FOO=1"])).targets,
            s(&["x.php", "FOO=1"])
        );
    }
}
