//! Az MCP-kliensek útmutatója a skill-utasítás forrásából (`skill/skill-i18n/SKILL.<nyelv>.md`, fordításkor
//! beágyazva), hogy a kettő ne csússzon szét:
//! - `instructions`: az `initialize` válaszába kerülő tömör útmutató — a forrás `<!-- mcp:<név> -->` …
//!   `<!-- /mcp -->` közé jelölt szakaszai a [`SECTIONS`] sorrendjében, a CLI-kapcsolók az MCP-eszköz / -paraméter
//!   nevére cserélve ([`CLI_TO_MCP`]);
//! - `manual`: a `guide` eszköz válasza — a teljes SKILL.md-törzs (frontmatter és jelölő-sorok nélkül).

use crate::i18n::Lang;

const SKILL_EN: &str = include_str!("../../skill/skill-i18n/SKILL.en.md");
const SKILL_HU: &str = include_str!("../../skill/skill-i18n/SKILL.hu.md");

/// A kivágott szakaszok, az útmutatóbeli sorrendjükben (eszközök elöl).
const SECTIONS: [&str; 4] = ["tools", "callers", "output", "limits"];

/// Egy jelölt szakasz vége.
const END: &str = "<!-- /mcp -->";

/// CLI-kapcsoló → MCP-eszköz / -paraméter (a jelölt szakaszokban, backtickkel együtt). A név szerinti hívó-minta
/// idézőjelei is lekerülnek: MCP-n a `pattern` sima JSON-string.
const CLI_TO_MCP: [(&str, &str); 13] = [
    ("`--grep`", "`find_callers`"),
    ("`--depth`", "`depth`"),
    ("`--strings`", "`search` (strings)"),
    ("`--const`", "`search` (constants)"),
    ("`--body`", "`method_body`"),
    ("`--raw`", "`raw: true`"),
    ("`--doc`", "`doc: true`"),
    ("`--guess`", "`guess: true`"),
    ("`--full`", "`full: true`"),
    ("`--project`", "`project`"),
    ("`--format`", "`format`"),
    ("`'(^|::|\\.|->)", "`(^|::|\\.|->)"),
    ("$'`", "$`"),
];

fn source(lang: Lang) -> &'static str {
    lang.pick(SKILL_EN, SKILL_HU)
}

/// Az `initialize` válaszának `instructions` mezője az adott nyelven.
pub(crate) fn instructions(lang: Lang) -> String {
    instructions_from(source(lang))
}

/// Az útmutató vége (`initialize`): a munkakönyvtár tömör projekt-profilja a nevekből (gyors, elemzés nélkül), vagy
/// ha a munkakönyvtár nem projekt (pl. Claude Desktop), egy tipp a `project_profile` eszközre. Soha nem bukik el.
pub(crate) fn startup_note(root: &std::path::Path, lang: Lang) -> String {
    match crate::profile::startup_summary(root, lang) {
        Some(profile) => {
            let mut text = lang
                .pick(
                    "\nProject profile of the working directory (from folder and file names; details: `project_profile`; narrow a query to one layer with `layer`):\n",
                    "\nA munkakönyvtár projekt-profilja (a mappa- és fájlnevekből; részletek: `project_profile`; egy rétegre szűkítés: `layer`):\n",
                )
                .to_owned();
            text.push_str(&profile);
            text
        }
        None => lang
            .pick(
                "\nIn an unfamiliar project call `project_profile` first (architecture, layers, patterns from folder and file names), then narrow queries with `layer`.\n",
                "\nIsmeretlen projektben előbb a `project_profile` (architektúra, rétegek, minták a mappa- és fájlnevekből), utána a lekérdezések szűkíthetők a `layer` paraméterrel.\n",
            )
            .to_owned(),
    }
}

/// A `guide` eszköz válasza: a teljes skill-útmutató az adott nyelven.
pub(crate) fn manual(lang: Lang) -> String {
    manual_from(source(lang))
}

fn instructions_from(source: &str) -> String {
    let source = source.replace("\r\n", "\n");
    let parts: Vec<&str> = SECTIONS
        .iter()
        .filter_map(|name| section(&source, name))
        .collect();
    let mut text = parts.join("\n\n");
    for (cli, mcp) in CLI_TO_MCP {
        text = text.replace(cli, mcp);
    }
    text.push('\n');
    text
}

/// A `<!-- mcp:<name> -->` és a következő `<!-- /mcp -->` közötti szöveg; `None`, ha a jelölő hiányzik vagy
/// többször szerepel.
fn section<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("<!-- mcp:{name} -->\n");
    let mut found = source.match_indices(start.as_str());
    let (at, _) = found.next()?;
    if found.next().is_some() {
        return None;
    }
    let body = &source[at + start.len()..];
    let end = body.find(END)?;
    Some(body[..end].trim_end())
}

fn manual_from(source: &str) -> String {
    let source = source.replace("\r\n", "\n");
    let body = source
        .strip_prefix("---\n")
        .and_then(|rest| {
            rest.find("\n---\n")
                .map(|end| &rest[end + "\n---\n".len()..])
        })
        .unwrap_or(source.as_str());
    let mut text = body
        .trim_start()
        .lines()
        .filter(|line| !is_marker(line))
        .collect::<Vec<_>>()
        .join("\n");
    text.push('\n');
    text
}

fn is_marker(line: &str) -> bool {
    line == END || (line.starts_with("<!-- mcp:") && line.ends_with(" -->"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOOLS: [&str; 8] = [
        "outline",
        "method_body",
        "find_callers",
        "search",
        "framework_view",
        "notes",
        "projects",
        "guide",
    ];

    #[test]
    fn every_section_is_marked_once_in_both_sources() {
        for src in [SKILL_EN, SKILL_HU] {
            for name in SECTIONS {
                let text = section(src, name).unwrap_or_else(|| panic!("section {name}"));
                assert!(!text.trim().is_empty(), "section {name} is empty");
                assert!(!text.contains("<!-- mcp:"), "nested marker in {name}");
            }
            // nincs más (elírt / árva) jelölő
            assert_eq!(src.matches("<!-- mcp:").count(), SECTIONS.len());
            assert_eq!(src.matches(END).count(), SECTIONS.len());
        }
    }

    #[test]
    fn instructions_are_compact_and_complete_in_both_languages() {
        for (lang, root) in [(Lang::En, "`root: "), (Lang::Hu, "`gyökér: ")] {
            let text = instructions(lang);
            let lines = text.lines().count();
            assert!((25..=70).contains(&lines), "{lang:?}: {lines} lines");
            assert!(text.len() <= 6500, "{lang:?}: {} bytes", text.len());
            for item in [
                "`(^|::|\\.|->)",
                root,
                "`via ",
                "`?::",
                "`raw: true`",
                "`doc: true`",
                "`depth`",
                "`guess: true`",
                "`full: true`",
                "400",
            ] {
                assert!(text.contains(item), "{lang:?}: missing {item}\n{text}");
            }
            for tool in TOOLS {
                assert!(text.contains(&format!("`{tool}`")), "{lang:?}: {tool}");
            }
            // csak MCP-nevek: CLI-kapcsoló, shell-parancs és idézett minta nem maradhat benne
            assert!(!text.contains("`--"), "{lang:?}: CLI flag left\n{text}");
            assert!(!text.contains("bash "), "{lang:?}: shell command left");
            assert!(!text.contains("`'"), "{lang:?}: quoted pattern left");
            assert!(!text.contains("<!--"), "{lang:?}: marker left");
        }
        assert_ne!(instructions(Lang::En), instructions(Lang::Hu));
    }

    #[test]
    fn both_languages_have_the_same_shape() {
        let en = instructions(Lang::En);
        let hu = instructions(Lang::Hu);
        let bullets = |s: &str| s.lines().filter(|l| l.starts_with("- ")).count();
        assert_eq!(bullets(&en), bullets(&hu));
        let tokens = |s: &str| {
            [
                "`raw: true`",
                "`doc: true`",
                "`guess: true`",
                "`full: true`",
                "`find_callers`",
                "`method_body`",
                "`search`",
            ]
            .map(|t| s.matches(t).count())
        };
        assert_eq!(tokens(&en), tokens(&hu));
    }

    #[test]
    fn startup_note_names_the_profile_tool_without_cli_flags() {
        let dir = std::env::temp_dir().join(format!("callgraph-guide-note-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app/Http/Controllers")).unwrap();
        // projekt-jel nélkül: csak a tipp
        for lang in Lang::ALL {
            let text = startup_note(&dir, lang);
            assert!(text.contains("`project_profile`"), "{lang:?}: {text}");
            assert!(!text.contains("`--"), "{lang:?}: {text}");
        }
        std::fs::write(dir.join("composer.json"), "{}").unwrap();
        std::fs::write(
            dir.join("app/Http/Controllers/UserController.php"),
            "<?php\nclass UserController {}\n",
        )
        .unwrap();
        let text = startup_note(&dir, Lang::En);
        assert!(text.contains("profile: "), "{text}");
        assert!(text.contains("controller"), "{text}");
        assert!(!text.contains("💡"), "{text}");
        assert!(text.lines().count() <= 18, "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn crlf_sources_give_the_same_text() {
        let crlf = SKILL_HU.replace('\n', "\r\n");
        assert_eq!(instructions_from(&crlf), instructions(Lang::Hu));
        assert_eq!(manual_from(&crlf), manual(Lang::Hu));
    }

    #[test]
    fn manual_is_the_skill_body_without_frontmatter_and_markers() {
        assert!(manual(Lang::En).starts_with("# callgraph — code map without Read\n"));
        assert!(manual(Lang::Hu).starts_with("# callgraph — kód-térkép Read nélkül\n"));
        for lang in Lang::ALL {
            let text = manual(lang);
            assert!(!text.contains("<!--"), "{lang:?}");
            assert!(!text.contains("\nname: callgraph"), "{lang:?}");
            assert!(text.contains("## MCP"), "{lang:?}");
            assert!(
                text.contains("`--grep`"),
                "{lang:?}: the long form keeps the CLI"
            );
            assert!(text.ends_with(".\n") || text.ends_with("`\n"), "{lang:?}");
        }
    }
}
