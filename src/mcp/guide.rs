//! Az MCP-kliensek útmutatója a skill-utasítás forrásából (fordításkor beágyazva), hogy a kettő ne csússzon szét.
//! A skill csak angolul létezik (`skill/SKILL.md` + `skill/ref/*.md`):
//! - `instructions`: az `initialize` válaszába kerülő tömör útmutató — a források `<!-- mcp:<név> -->` …
//!   `<!-- /mcp -->` közé jelölt szakaszai a [`SECTIONS`] sorrendjében, a CLI-kapcsolók az MCP-eszköz / -paraméter
//!   nevére cserélve ([`CLI_TO_MCP`]). A források: az alap SKILL.md (`start`, `callers`, `output`), a `ref/mcp.md`
//!   (`tools`) és a `ref/reference.md` (`limits`);
//! - `manual`: a `guide` eszköz válasza téma nélkül — az alap SKILL.md-törzs (frontmatter és jelölő-sorok nélkül);
//! - `topic`: a `guide` eszköz válasza `topic`-kal — pontosan az adott `ref/*.md` fájl.

use crate::i18n::Lang;

const SKILL: &str = include_str!("../../skill/SKILL.md");

/// A `guide` témái: név → a beágyazott `skill/ref/<név>.md`.
const TOPICS: [(&str, &str); 13] = [
    ("php", include_str!("../../skill/ref/php.md")),
    ("blade", include_str!("../../skill/ref/blade.md")),
    ("js", include_str!("../../skill/ref/js.md")),
    ("vue", include_str!("../../skill/ref/vue.md")),
    ("css", include_str!("../../skill/ref/css.md")),
    ("rust", include_str!("../../skill/ref/rust.md")),
    ("swift", include_str!("../../skill/ref/swift.md")),
    ("kotlin", include_str!("../../skill/ref/kotlin.md")),
    ("python", include_str!("../../skill/ref/python.md")),
    ("go", include_str!("../../skill/ref/go.md")),
    ("gen", include_str!("../../skill/ref/gen.md")),
    ("mcp", include_str!("../../skill/ref/mcp.md")),
    ("reference", include_str!("../../skill/ref/reference.md")),
];

/// A kivágott szakaszok, az útmutatóbeli sorrendjükben (a munkamegosztás és a `locate` elöl, utána az eszközök).
const SECTIONS: [&str; 5] = ["start", "tools", "callers", "output", "limits"];

/// Egy jelölt szakasz vége.
const END: &str = "<!-- /mcp -->";

/// CLI-kapcsoló → MCP-eszköz / -paraméter (a jelölt szakaszokban, backtickkel együtt). A név szerinti hívó-minta
/// idézőjelei is lekerülnek: MCP-n a `pattern` sima JSON-string.
const CLI_TO_MCP: [(&str, &str); 15] = [
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
    ("`--all`", "`all: true`"),
    ("`profile`", "`project_profile`"),
    ("`'(^|::|\\.|->)", "`(^|::|\\.|->)"),
    ("$'`", "$`"),
];

fn topic_source(name: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(topic, _)| *topic == name)
        .map(|(_, text)| *text)
}

/// Az összes jelölt szakasz forrása: az alap és a szakaszokat tartalmazó két referencia-fájl.
fn marked_sources() -> String {
    let part = |name| topic_source(name).unwrap_or_default();
    format!("{SKILL}\n{}\n{}", part("mcp"), part("reference"))
}

/// Az `initialize` válaszának `instructions` mezője.
pub(crate) fn instructions() -> String {
    instructions_from(&marked_sources())
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

/// A `guide` eszköz válasza téma nélkül: az alap skill-útmutató.
pub(crate) fn manual() -> String {
    manual_from(SKILL)
}

/// A `guide` eszköz válasza `topic`-kal: pontosan a téma `ref/*.md` fájlja; ismeretlen témára hibaüzenet.
pub(crate) fn topic(name: &str) -> Result<String, String> {
    topic_source(name).map(str::to_owned).ok_or_else(|| {
        let names: Vec<&str> = TOPICS.iter().map(|(topic, _)| *topic).collect();
        format!("unknown topic `{name}`; available: {}", names.join(", "))
    })
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

    const TOOLS: [&str; 10] = [
        "locate",
        "outline",
        "method_body",
        "find_callers",
        "search",
        "framework_view",
        "notes",
        "projects",
        "guide",
        "next",
    ];

    #[test]
    fn every_section_is_marked_once() {
        let src = marked_sources();
        for name in SECTIONS {
            let text = section(&src, name).unwrap_or_else(|| panic!("section {name}"));
            assert!(!text.trim().is_empty(), "section {name} is empty");
            assert!(!text.contains("<!-- mcp:"), "nested marker in {name}");
        }
        // nincs más (elírt / árva) jelölő
        assert_eq!(src.matches("<!-- mcp:").count(), SECTIONS.len());
        assert_eq!(src.matches(END).count(), SECTIONS.len());
    }

    #[test]
    fn instructions_are_compact_and_complete() {
        let text = instructions();
        let lines = text.lines().count();
        assert!((25..=70).contains(&lines), "{lines} lines");
        assert!(text.len() <= 6500, "{} bytes", text.len());
        for item in [
            "`(^|::|\\.|->)",
            "`root: ",
            "`via ",
            "`?::",
            "`raw: true`",
            "`doc: true`",
            "`depth`",
            "`guess: true`",
            "`full: true`",
            "400",
        ] {
            assert!(text.contains(item), "missing {item}\n{text}");
        }
        for tool in TOOLS {
            assert!(text.contains(&format!("`{tool}`")), "{tool}");
        }
        // csak MCP-nevek: CLI-kapcsoló, shell-parancs és idézett minta nem maradhat benne
        assert!(!text.contains("`--"), "CLI flag left\n{text}");
        assert!(!text.contains("bash "), "shell command left");
        assert!(!text.contains("`'"), "quoted pattern left");
        assert!(!text.contains("<!--"), "marker left");
    }

    #[test]
    fn start_section_carries_the_recipes_with_mcp_names() {
        let text = instructions();
        let marker = "**Recipes:**";
        let recipes = text
            .lines()
            .skip_while(|line| !line.starts_with(marker))
            .take(3)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(recipes.starts_with(marker), "no recipes\n{text}");
        // a négy recept eszköz-nevei (bug fix, hívók, ismeretlen projekt, szöveg / konstans)
        for tool in [
            "`locate`",
            "`outline`",
            "`method_body`",
            "`find_callers`",
            "`depth`",
            "`project_profile`",
            "`search` (strings)",
            "`search` (constants)",
        ] {
            assert!(recipes.contains(tool), "{tool}\n{recipes}");
        }
        assert!(!recipes.contains("--"), "CLI flag left\n{recipes}");
        assert!(!recipes.contains("bash "), "shell command left");
        assert!(recipes.contains("Grep"), "exact text goes to Grep");
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
        let crlf = marked_sources().replace('\n', "\r\n");
        assert_eq!(instructions_from(&crlf), instructions());
        assert_eq!(manual_from(&SKILL.replace('\n', "\r\n")), manual());
    }

    #[test]
    fn topics_match_the_reference_directory_and_the_router() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skill/ref");
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        let mut names: Vec<String> = TOPICS.iter().map(|(n, _)| format!("{n}.md")).collect();
        names.sort();
        assert_eq!(files, names);
        for (name, _) in TOPICS {
            assert!(SKILL.contains(&format!("(ref/{name}.md)")), "{name}");
            assert!(
                SKILL.contains(&format!("`{name}`")),
                "{name} in the guide hint"
            );
        }
        assert!(SKILL.contains("guide {topic:"), "the MCP hint");
    }

    #[test]
    fn every_topic_returns_exactly_its_file_starting_with_its_heading() {
        for (name, _) in TOPICS {
            let text = topic(name).unwrap_or_else(|e| panic!("{name}: {e}"));
            let file = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("skill/ref")
                    .join(format!("{name}.md")),
            )
            .unwrap();
            assert_eq!(
                text.replace("\r\n", "\n"),
                file.replace("\r\n", "\n"),
                "{name}"
            );
            assert!(text.starts_with("# callgraph — "), "{name}: {text}");
            assert!(text.len() > 200, "{name}");
        }
    }

    #[test]
    fn unknown_topic_is_an_error_listing_the_topics() {
        let err = topic("cobol").unwrap_err();
        assert!(err.contains("unknown topic `cobol`"), "{err}");
        for (name, _) in TOPICS {
            assert!(err.contains(name), "{err}");
        }
        assert!(topic("").is_err());
        assert!(topic("Php").is_err());
    }

    #[test]
    fn manual_is_the_core_body_without_frontmatter_and_markers() {
        let text = manual();
        assert!(text.starts_with("# callgraph — code map without Read\n"));
        assert!(!text.contains("<!--"));
        assert!(!text.contains("\nname: callgraph"));
        assert!(text.contains("## Read next"));
        assert!(text.contains("`--grep`"), "the long form keeps the CLI");
        assert!(
            text.len() <= 15_000,
            "the core manual is {} bytes",
            text.len()
        );
        assert!(text.ends_with('\n') && !text.ends_with("\n\n"));
    }
}
