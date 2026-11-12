#!/usr/bin/env python3
"""Static site generator for https://callgraph.eu (Python 3 stdlib only).

Page bodies live in tools/site/<lang>/<key>.html (HTML fragments for the inside of <main>);
this script wraps them in the shared template (head with SEO tags, nav, breadcrumb, footer),
renders the changelog from the repository's CHANGELOG.md (user-facing entries only) and writes every page plus sitemap.xml
into the repository root.

    python3 tools/build.py            # build
    python3 tools/check.py            # verify the result

In the fragments, a fenced block is escaped and wrapped for you:

    ```            -> <pre class="code"><code>…</code></pre>
    ```out         -> <pre class="code out"><code>…</code></pre>   (example output)

and {{version}} / {{faq}} are replaced.
"""
import html
import json
import os
import re
import sys
from html.parser import HTMLParser

HERE = os.path.dirname(os.path.abspath(__file__))
SITE_DIR = os.path.join(HERE, "content")
CHANGELOG = os.path.join(os.path.dirname(HERE), "CHANGELOG.md")
# the Hungarian changelog: same releases and sections, entries in Hungarian (technical terms stay English)
CHANGELOG_HU = os.path.join(os.path.dirname(HERE), "CHANGELOG.hu.md")
# the output folder (the gh-pages checkout): python3 site/build.py <out-dir>
ROOT = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else None

# The website is user documentation only: changelog entries about the project's own build, CI, packaging,
# repository layout or internals are left out (the full CHANGELOG.md stays on GitHub).
DEV_ENTRY = re.compile(r"tools/dev\.sh|\bclippy\b|gh-pages|skill-i18n|releases/latest|GitHub API|"
                       r"committed / packaged|rust-cache|\bLTO\b|Docker", re.I)
BASE = "https://callgraph.eu"
REPO = "https://github.com/GeRiY/callgraph"
LANGS = ("en", "hu")
OG_IMAGE = BASE + "/assets/og-image.png"

# --------------------------------------------------------------------------- pages
# key: (en path, hu path), per-language title / description / breadcrumb label.
PAGES = [
    {"key": "home", "path": {"en": "/", "hu": "/hu/"}, "priority": "1.0",
     "title": {"en": "callgraph — Code map for AI coding agents",
               "hu": "callgraph — kód-térkép AI kódoló agenteknek"},
     "desc": {"en": "Show AI agents a file's structure, call graph and callers without reading it. PHP, JS/TS, Vue, Rust, Swift. Free, MIT, one binary: CLI or MCP server.",
              "hu": "Szerkezet, hívás-gráf és hívók AI agenteknek a fájl beolvasása nélkül. PHP, JS/TS, Vue, Rust, Swift. Ingyenes, MIT, egy bináris: CLI vagy MCP-szerver."},
     "crumb": {"en": "Home", "hu": "Kezdőlap"}},
    {"key": "mcp", "path": {"en": "/mcp/", "hu": "/hu/mcp/"}, "priority": "0.9",
     "title": {"en": "callgraph MCP server setup for Claude Code, Cursor, VS Code",
               "hu": "callgraph MCP-szerver: Claude Code, Cursor, VS Code"},
     "desc": {"en": "Run callgraph as a local MCP server: setup for Claude Code, Claude Desktop, Cursor and VS Code, --auto-update, the ten tools and troubleshooting.",
              "hu": "A callgraph helyi MCP-szerverként: beállítás Claude Code-hoz, Claude Desktophoz, Cursorhoz és VS Code-hoz, --auto-update, a tíz eszköz, hibakeresés."},
     "crumb": {"en": "MCP server", "hu": "MCP-szerver"}},
    {"key": "php", "path": {"en": "/php/", "hu": "/hu/php/"}, "priority": "0.8",
     "title": {"en": "PHP call graph & impact analysis for AI agents — callgraph",
               "hu": "PHP call graph és hatáselemzés AI agenteknek — callgraph"},
     "desc": {"en": "Outline PHP classes, methods and categorized calls, find every caller and run Laravel / Yii views (routes, validation, events) without reading files.",
              "hu": "PHP-osztályok, metódusok és kategorizált hívások, minden hívó és Laravel / Yii nézetek (routes, validation, events) a fájlok beolvasása nélkül."},
     "crumb": {"en": "PHP", "hu": "PHP"}},
    {"key": "rust", "path": {"en": "/rust/", "hu": "/hu/rust/"}, "priority": "0.8",
     "title": {"en": "Rust call graph & impact analysis for AI agents — callgraph",
               "hu": "Rust call graph és hatáselemzés AI agenteknek — callgraph"},
     "desc": {"en": "Map Rust crates for AI agents: structs, traits, impl blocks, use trees and macro calls, framework detection from Cargo.toml and who calls what.",
              "hu": "Rust-crate-ek térképe AI agenteknek: structok, traitek, impl-blokkok, use-fák, makróhívások, framework a Cargo.toml-ból, és hogy ki mit hív."},
     "crumb": {"en": "Rust", "hu": "Rust"}},
    {"key": "typescript", "path": {"en": "/typescript/", "hu": "/hu/typescript/"}, "priority": "0.8",
     "title": {"en": "TypeScript, JavaScript & Vue call graph — callgraph",
               "hu": "TypeScript, JavaScript és Vue call graph — callgraph"},
     "desc": {"en": "Outline JS, TS and Vue single-file components: imports with alias resolution, props, emits, setup, template components and typed-receiver calls.",
              "hu": "JS, TS és Vue SFC outline: importok alias-feloldással, props, emits, setup, template-komponensek, típusos fogadón át hívott metódusok és hívóik."},
     "crumb": {"en": "TypeScript / Vue", "hu": "TypeScript / Vue"}},
    {"key": "swift", "path": {"en": "/swift/", "hu": "/hu/swift/"}, "priority": "0.8",
     "title": {"en": "Swift & SwiftUI call graph for AI agents — callgraph",
               "hu": "Swift és SwiftUI call graph AI agenteknek — callgraph"},
     "desc": {"en": "Map Swift and SwiftUI code: View types as components, body and computed properties, property wrappers, SwiftUI views and modifiers as call categories.",
              "hu": "Swift és SwiftUI térkép: View-típusok komponensként, body és computed property-k, property wrapperek, SwiftUI view-k és modifierek kategóriaként."},
     "crumb": {"en": "Swift / SwiftUI", "hu": "Swift / SwiftUI"}},
    {"key": "locate", "path": {"en": "/locate/", "hu": "/hu/locate/"}, "priority": "0.8",
     "title": {"en": "Where to start with a coding task: locate — callgraph",
               "hu": "Honnan induljon egy kódolási feladat: locate — callgraph"},
     "desc": {"en": "callgraph locate takes the names and terms of a task in one call and ranks the few files worth opening first, each with the reason. When to use grep.",
              "hu": "A callgraph locate egy hívásban kapja a feladat neveit és fogalmait, és rangsorolja a először megnyitandó fájlokat, indoklással. Mikor jobb a grep."},
     "crumb": {"en": "locate", "hu": "locate"}},
    {"key": "profile", "path": {"en": "/profile/", "hu": "/hu/profile/"}, "priority": "0.8",
     "title": {"en": "Project profile: architecture & layers — callgraph",
               "hu": "Projekt-profil: architektúra és rétegek — callgraph"},
     "desc": {"en": "callgraph profile describes an unfamiliar project in about 15 lines: architecture, layers and folders, patterns, call direction. Plus --layer filters.",
              "hu": "A callgraph profile kb. 15 sorban leírja az ismeretlen projektet: architektúra, rétegek és mappák, minták, hívásirány. Hozzá a --layer szűrők."},
     "crumb": {"en": "Project profile", "hu": "Projekt-profil"}},
    {"key": "changelog", "path": {"en": "/changelog/", "hu": "/hu/changelog/"}, "priority": "0.6",
     "title": {"en": "Changelog and release notes — callgraph",
               "hu": "Változásnapló és kiadási jegyzetek — callgraph"},
     "desc": {"en": "Every callgraph release with its date: what was added, changed and fixed, in plain language. Also as an Atom feed; downloads on GitHub releases.",
              "hu": "Minden callgraph-kiadás dátummal: mi új, mi változott és mi javult, közérthetően. Atom-feed is van; a letöltések a GitHub kiadási oldalán."},
     "crumb": {"en": "Changelog", "hu": "Változásnapló"}},
]
BY_KEY = {p["key"]: p for p in PAGES}

UI = {
    "en": {"home": "Home", "langs": "Languages", "profile": "Profile", "changelog": "Changelog",
           "nav": "Main", "crumbs": "Breadcrumb", "skip": "Skip to content", "switch": "Magyar",
           "made": "made by", "license": "MIT license", "releases": "Releases", "author": "Gergő Makra",
           "locale": "en_US", "other_locale": "hu_HU", "mcp": "MCP", "pages": "Pages",
           "usage": "Usage", "usage_all": "All commands", "langs_all": "All languages",
           "copy": "Copy", "copied": "Copied", "copy_label": "Copy code to clipboard",
           "md": "Markdown version of this page"},
    "hu": {"home": "Kezdőlap", "langs": "Nyelvek", "profile": "Profil", "changelog": "Változásnapló",
           "nav": "Fő navigáció", "crumbs": "Morzsamenü", "skip": "Ugrás a tartalomra", "switch": "English",
           "made": "készítette:", "license": "MIT licenc", "releases": "Kiadások", "author": "Makra Gergő",
           "locale": "hu_HU", "other_locale": "en_US", "mcp": "MCP", "pages": "Oldalak",
           "usage": "Használat", "usage_all": "Minden parancs", "langs_all": "Minden nyelv",
           "copy": "Másolás", "copied": "Másolva", "copy_label": "Kód másolása a vágólapra",
           "md": "Az oldal Markdown-változata"},
}
LANG_PAGES = [("php", "PHP"), ("rust", "Rust"), ("typescript", "TS / Vue"), ("swift", "Swift")]

FAQ = {
    "en": [
        ("What is callgraph?", "A command-line code map for AI coding agents. It parses a file or folder and answers what is in it, which method lives on which lines, what it calls, who calls it and what is affected by a change — in a few dozen lines instead of the whole file."),
        ("Which languages are supported?", 'PHP (with Laravel and Yii awareness), JavaScript, TypeScript, Vue single-file components, Rust, Swift and SwiftUI, CSS, SCSS, JSON and Markdown. See the <a href="/php/">PHP</a>, <a href="/rust/">Rust</a>, <a href="/typescript/">TypeScript / Vue</a> and <a href="/swift/">Swift</a> pages.'),
        ("Do I need PHP, Node or Rust installed?", "No. callgraph is a single static binary. The only runtime requirement is bash."),
        ("Does it work without Claude Code?", 'Yes. It ships as a Claude Code skill, but every command works from any terminal or script, and any MCP client (Claude Desktop, Cursor, VS Code…) can use it through <code>callgraph mcp</code> — see <a href="/mcp/">MCP setup</a>.'),
        ("Is my code sent anywhere?", "No. Everything runs locally; your code is never uploaded. The only network call is the update check against GitHub Releases (at most once a day; off with <code>CALLGRAPH_UPDATE_CHECK=0</code>)."),
        ("How do I update it?", 'Run <code>callgraph.sh update</code>. It downloads the newest release from GitHub, checks it against the release\'s <code>SHA256SUMS</code> and keeps your projects and settings. What changed: <a href="/changelog/">changelog</a>.'),
        ("Where do I report a bug or suggest a feature?", 'On GitHub: <a href="https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml">bug report</a> or <a href="https://github.com/GeRiY/callgraph/issues/new?template=feature_request.yml">idea / feedback</a>. Security issues go to a <a href="https://github.com/GeRiY/callgraph/security/advisories/new">private advisory</a>.'),
        ("How much does it cost?", "Nothing. callgraph is open source under the MIT license."),
    ],
    "hu": [
        ("Mi a callgraph?", "Parancssori kód-térkép AI kódoló agenteknek. Elemzi a fájlt vagy mappát, és megmondja, mi van benne, melyik metódus melyik sorokon él, mit hív, ki hívja, és mit érint egy változtatás — pár tucat sorban a teljes fájl helyett."),
        ("Milyen nyelveket támogat?", 'PHP (Laravel- és Yii-ismerettel), JavaScript, TypeScript, Vue single-file komponensek, Rust, Swift és SwiftUI, CSS, SCSS, JSON és Markdown. Részletek: <a href="/hu/php/">PHP</a>, <a href="/hu/rust/">Rust</a>, <a href="/hu/typescript/">TypeScript / Vue</a>, <a href="/hu/swift/">Swift</a>.'),
        ("Kell hozzá PHP, Node vagy Rust?", "Nem. A callgraph egyetlen statikus bináris, futtatáshoz csak bash kell."),
        ("Működik Claude Code nélkül is?", 'Igen. Claude Code skillként érkezik, de minden parancs bármilyen terminálból vagy szkriptből használható, és bármely MCP-kliens (Claude Desktop, Cursor, VS Code…) is használhatja a <code>callgraph mcp</code>-vel — lásd az <a href="/hu/mcp/">MCP-beállítást</a>.'),
        ("Elküldi valahova a kódomat?", "Nem. Minden helyben fut; a kódod sehova nem töltődik fel. Az egyetlen hálózati hívás a frissítés-ellenőrzés a GitHub Release-ek felé (legfeljebb naponta egyszer; kikapcsolás: <code>CALLGRAPH_UPDATE_CHECK=0</code>)."),
        ("Hogyan frissíthető?", 'A <code>callgraph.sh update</code> letölti a legfrissebb kiadást a GitHubról, ellenőrzi a kiadás <code>SHA256SUMS</code> fájlja szerint, a projektek és a beállítások megmaradnak. Mi változott: <a href="/hu/changelog/">változásnapló</a>.'),
        ("Hol jelezhetek hibát vagy kérhetek új funkciót?", 'A GitHubon: <a href="https://github.com/GeRiY/callgraph/issues/new?template=bug_report.yml">hibajelentés</a> vagy <a href="https://github.com/GeRiY/callgraph/issues/new?template=feature_request.yml">ötlet / észrevétel</a>. Biztonsági hibát <a href="https://github.com/GeRiY/callgraph/security/advisories/new">privátban</a> jelezz.'),
        ("Mennyibe kerül?", "Semmibe. A callgraph nyílt forrású, MIT licenccel."),
    ],
}

INSTALL_ONELINER = ('p="$(uname -s | tr \'[:upper:]\' \'[:lower:]\')-$(uname -m)" && t="$(mktemp -d)" '
                    '&& curl -fsSL -o "$t/cg.zip" "https://update.callgraph.eu/v1/download/latest/callgraph-$p.zip?k=install" '
                    '&& mkdir -p ~/.claude/skills && unzip -qo "$t/cg.zip" -d ~/.claude/skills '
                    '&& bash ~/.claude/skills/callgraph/install.sh && rm -rf "$t"')
MCP_ADD_UNIX = ("claude mcp add -s user callgraph -- ~/.claude/skills/callgraph/bin/"
                "$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)/callgraph mcp --auto-update")
MCP_ADD_WIN = ('claude mcp add -s user callgraph -- "$env:USERPROFILE\\.claude\\skills\\callgraph\\bin\\'
               'windows-x86_64\\callgraph.exe" mcp --auto-update')

HOWTO = {
    "install": {
        "en": {"name": "Install callgraph on macOS or Linux",
               "description": "Install the callgraph Claude Code skill and binary, register a project and restart Claude Code.",
               "steps": [("Download and install the package", "Run in a terminal: " + INSTALL_ONELINER),
                         ("Register your project once", "bash ~/.claude/skills/callgraph/projects.sh add shop /absolute/path/to/shop"),
                         ("Restart Claude Code", "Restart Claude Code so it picks up the skill (SKILL.md).")]},
        "hu": {"name": "A callgraph telepítése macOS-en vagy Linuxon",
               "description": "A callgraph Claude Code skill és bináris telepítése, egy projekt regisztrálása és a Claude Code újraindítása.",
               "steps": [("A csomag letöltése és telepítése", "Terminálban futtasd: " + INSTALL_ONELINER),
                         ("A projekt egyszeri regisztrálása", "bash ~/.claude/skills/callgraph/projects.sh add shop /abszolút/út/a/shophoz"),
                         ("A Claude Code újraindítása", "Indítsd újra a Claude Code-ot, hogy betöltse a skillt (SKILL.md).")]},
    },
    "mcp": {
        "en": {"name": "Register the callgraph MCP server in Claude Code",
               "description": "Add callgraph as a user-level MCP server in Claude Code, with automatic updates.",
               "steps": [("Install the callgraph package", "Install the skill package first; it contains the binary for your platform (see callgraph.eu/#install)."),
                         ("Add the MCP server", "macOS / Linux: " + MCP_ADD_UNIX + " — Windows (PowerShell): " + MCP_ADD_WIN),
                         ("Restart Claude Code", "Restart Claude Code; the callgraph tools are then available in every project."),
                         ("Optional: register project aliases", "bash ~/.claude/skills/callgraph/projects.sh add shop /path/to/shop")]},
        "hu": {"name": "A callgraph MCP-szerver felvétele a Claude Code-ba",
               "description": "A callgraph felvétele felhasználói szintű MCP-szerverként a Claude Code-ba, automatikus frissítéssel.",
               "steps": [("A callgraph-csomag telepítése", "Előbb telepítsd a skill-csomagot; ebben van a géped binárisa (lásd callgraph.eu/hu/#install)."),
                         ("Az MCP-szerver felvétele", "macOS / Linux: " + MCP_ADD_UNIX + " — Windows (PowerShell): " + MCP_ADD_WIN),
                         ("A Claude Code újraindítása", "Indítsd újra a Claude Code-ot; a callgraph eszközei ezután minden projektben elérhetők."),
                         ("Opcionális: projekt-aliasok", "bash ~/.claude/skills/callgraph/projects.sh add shop /út/a/shophoz")]},
    },
}

# --------------------------------------------------------------------------- helpers
esc = html.escape


def url(path):
    return BASE + path


def counterpart(page, lang):
    return page["path"]["hu" if lang == "en" else "en"]


def fences(text):
    def repl(m):
        cls = "code out" if m.group(1) == "out" else "code"
        return '<pre class="%s"><code>%s</code></pre>' % (cls, esc(m.group(2), quote=False))
    return re.sub(r"^```(\w*)\n(.*?)\n```$", repl, text, flags=re.S | re.M)


def strip_tags(s):
    return html.unescape(re.sub(r"<[^>]+>", "", s))


def ld(obj):
    return '<script type="application/ld+json">\n%s\n</script>' % json.dumps(obj, ensure_ascii=False)


# --------------------------------------------------------------------------- changelog
def parse_changelog(path):
    text = open(path, encoding="utf-8").read()
    links = dict(re.findall(r"^\[([^\]]+)\]:\s*(\S+)\s*$", text, re.M))
    releases = []
    cur = None
    sec = None
    for line in text.splitlines():
        m = re.match(r"^## \[([^\]]+)\](?:\s*[—-]\s*(\S+))?", line)
        if m:
            cur = None
            if m.group(1).lower() == "unreleased":
                continue
            cur = {"version": m.group(1), "date": m.group(2) or "", "url": links.get(m.group(1), REPO + "/releases"),
                   "blocks": []}
            releases.append(cur)
            sec = None
            continue
        if cur is None or re.match(r"^\[[^\]]+\]:", line):
            continue
        m = re.match(r"^### (.+)", line)
        if m:
            sec = {"title": m.group(1).strip(), "items": [], "paras": []}
            cur["blocks"].append(sec)
            continue
        if sec is None:
            sec = {"title": None, "items": [], "paras": []}
            cur["blocks"].append(sec)
        if line.startswith("- "):
            sec["items"].append(line[2:].strip())
        elif line.startswith("  ") and line.strip() and sec["items"]:
            sec["items"][-1] += " " + line.strip()
        elif line.strip():
            sec["paras"].append(line.strip())
    return releases


def md_inline(s):
    codes = []

    def keep(m):
        codes.append("<code>%s</code>" % esc(m.group(1), quote=False))
        return "\x00%d\x00" % (len(codes) - 1)
    s = re.sub(r"`([^`]+)`", keep, s)
    s = esc(s, quote=False)
    s = re.sub(r"\*\*(.+?)\*\*", r"<b>\1</b>", s)
    s = re.sub(r"\[([^\]]+)\]\((https?://[^)\s]+)\)", r'<a href="\2">\1</a>', s)
    s = re.sub(r"(?<![\"=>])(https://[^\s<)]+[^\s<).,])", r'<a href="\1">\1</a>', s)
    return re.sub(r"\x00(\d+)\x00", lambda m: codes[int(m.group(1))], s)


SECTION_HU = {"Added": "Új", "Changed": "Változás", "Fixed": "Javítás"}


def changelog_body(lang, releases):
    if lang == "en":
        out = ['<section class="hero page" aria-labelledby="title"><div>',
               '<h1 id="title">callgraph changelog</h1>',
               '<p class="lead">Every release with its date, newest first: what was added, changed and fixed. '
               'Downloads and checksums for each version are on the '
               '<a href="https://github.com/GeRiY/callgraph/releases">GitHub releases page</a>.</p>',
               '<div class="cta"><a class="btn primary" href="/#install">Install the latest version</a>'
               '<a class="btn" href="https://github.com/GeRiY/callgraph/releases/latest">Latest release on GitHub</a></div>',
               '</div></section>']
        rel_label, gh_label = "Releases", "Release notes and downloads on GitHub"
    else:
        out = ['<section class="hero page" aria-labelledby="title"><div>',
               '<h1 id="title">callgraph változásnapló</h1>',
               '<p class="lead">Minden kiadás dátummal, a legújabb elöl: mi új, mi változott és mi javult. '
               'A verziók letöltései és ellenőrzőösszegei a '
               '<a href="https://github.com/GeRiY/callgraph/releases">GitHub kiadási oldalán</a> vannak.</p>',
               '<div class="cta"><a class="btn primary" href="/hu/#install">A legújabb verzió telepítése</a>'
               '<a class="btn" href="https://github.com/GeRiY/callgraph/releases/latest">Legfrissebb kiadás a GitHubon</a></div>',
               '</div></section>']
        rel_label, gh_label = "Kiadások", "Kiadási jegyzet és letöltések a GitHubon"
    out.append('<nav class="toc" aria-label="%s"><ul>%s</ul></nav>' % (
        rel_label, "".join('<li><a href="#v%s">v%s</a></li>' % (r["version"], r["version"]) for r in releases)))
    for r in releases:
        rid = "v" + r["version"]
        out.append('<section id="%s" class="release" aria-labelledby="h-%s">' % (rid, rid))
        date = ' <span class="date">— <time datetime="%s">%s</time></span>' % (r["date"], r["date"]) if r["date"] else ""
        out.append('<h2 id="h-%s">v%s%s</h2>' % (rid, r["version"], date))
        out.append('<p class="sub"><a href="%s">%s</a></p>' % (esc(r["url"]), gh_label))
        for b in r["blocks"]:
            attr = ""
            if b["title"]:
                title = SECTION_HU.get(b["title"], b["title"]) if lang == "hu" else b["title"]
                out.append("<h3>%s</h3>" % esc(title))
            for p in b["paras"]:
                out.append("<p%s>%s</p>" % (attr, md_inline(p)))
            if b["items"]:
                out.append("<ul%s>%s</ul>" % (attr, "".join("<li>%s</li>" % md_inline(i) for i in b["items"])))
        out.append("</section>")
    return "\n".join(out) + "\n"


# --------------------------------------------------------------------------- template parts
def nav(page, lang):
    u = UI[lang]
    pre = "" if lang == "en" else "/hu"
    cur = page["key"] if page else None

    def a(key, label):
        p = BY_KEY[key]["path"][lang]
        ac = ' aria-current="page"' if key == cur else ""
        return '<a href="%s"%s>%s</a>' % (p, ac, esc(label))
    def menu(label, current, links):
        return '<details class="menu"><summary%s>%s</summary><div class="menu-list">%s</div></details>' % (
            ' class="current"' if current else "", esc(label), "".join(links))
    usage = ['<a href="%s/#usage">%s</a>' % (pre, esc(u["usage_all"])), a("locate", "locate"),
             a("profile", u["profile"])]
    items = [menu(u["usage"], cur in ("locate", "profile"), usage),
             menu(u["langs"], cur in dict(LANG_PAGES), [a(k, lbl) for k, lbl in LANG_PAGES] +
                  ['<a href="%s/#languages">%s</a>' % (pre, esc(u["langs_all"]))]),
             a("mcp", "MCP"), a("changelog", u["changelog"]),
             '<a href="%s">GitHub</a>' % REPO]
    other = "hu" if lang == "en" else "en"
    target = page["path"][other] if page else ("/hu/" if lang == "en" else "/")
    items.append('<a class="lang" href="%s" hreflang="%s" lang="%s">%s</a>' % (target, other, other, u["switch"]))
    home_cur = ' aria-current="page"' if cur == "home" else ""
    return ('<header class="top">\n  <a class="brand" href="%s/"%s><img src="/assets/favicon.svg" alt="" width="32" height="32">callgraph</a>\n'
            '  <nav class="links" aria-label="%s">\n    %s\n  </nav>\n</header>' % (pre, home_cur, esc(u["nav"]), "\n    ".join(items)))


def breadcrumb_html(page, lang):
    if page["key"] == "home":
        return ""
    u = UI[lang]
    return ('<nav class="crumbs" aria-label="%s"><ol><li><a href="%s">%s</a></li><li aria-current="page">%s</li></ol></nav>'
            % (esc(u["crumbs"]), BY_KEY["home"]["path"][lang], esc(u["home"]), esc(page["crumb"][lang])))


def breadcrumb_ld(page, lang):
    items = [{"@type": "ListItem", "position": 1, "name": "callgraph" if page["key"] == "home" else UI[lang]["home"],
              "item": url(BY_KEY["home"]["path"][lang])}]
    if page["key"] != "home":
        items.append({"@type": "ListItem", "position": 2, "name": page["crumb"][lang], "item": url(page["path"][lang])})
    return {"@context": "https://schema.org", "@type": "BreadcrumbList", "itemListElement": items}


def footer(page, lang):
    u = UI[lang]
    links = [(BY_KEY[k]["path"][lang], lbl) for k, lbl in
             [("home", u["home"]), ("mcp", "MCP"), ("php", "PHP"), ("rust", "Rust"), ("typescript", "TypeScript / Vue"),
              ("swift", "Swift"), ("locate", "locate"), ("profile", u["profile"]), ("changelog", u["changelog"])]]
    other = "hu" if lang == "en" else "en"
    target = page["path"][other] if page else ("/hu/" if lang == "en" else "/")
    return ('<footer>\n  <nav class="foot" aria-label="%s">%s</nav>\n'
            '  <div>callgraph · %s <a href="https://www.linkedin.com/in/gergomakra/" rel="author me">%s</a> · %s · '
            '<a href="%s">GitHub</a> · <a href="%s/releases/latest">%s</a> · '
            '<a href="%s" hreflang="%s" lang="%s">%s</a></div>\n'
            '  <div class="sub">%s</div>\n</footer>'
            % (esc(u["pages"]), " · ".join('<a href="%s">%s</a>' % (p, esc(l)) for p, l in links),
               u["made"], u["author"], u["license"], REPO, REPO, u["releases"], target, other, other, u["switch"],
               PRIVACY_NOTE[lang]))


PRIVACY_NOTE = {
    "en": "Privacy: no cookies. Page views are counted per day (page, referring site, download clicks) — no IP address, "
          "no identifiers. Controller: Makra Gergő, mgeri1993@gmail.com.",
    "hu": "Adatvédelem: nincs cookie. Az oldalmegtekintéseket napi számként rögzítjük (oldal, hivatkozó oldal, "
          "letöltés-kattintás) — IP-cím és azonosító nélkül. Adatkezelő: Makra Gergő, mgeri1993@gmail.com.",
}

# Page-view counter: no cookies, no storage, no identifiers — only the page path, the referring site's domain (or a
# `?ref=` / `utm_source=` source tag from a shared link) and
# whether a download link was clicked, counted per day on telemetry.callgraph.eu (see server/telemetry/).
SCRIPT_COUNT = """<script>
(function(){try{var u="https://telemetry.callgraph.eu/hit",r="";
try{var h=new URL(document.referrer).hostname;if(h&&h.indexOf("callgraph.eu")<0)r=h.toLowerCase()}catch(e){}
try{var q=new URLSearchParams(location.search),t=q.get("ref")||q.get("utm_source");if(t)r=t.toLowerCase().replace(/[^a-z0-9.-]/g,"").slice(0,64)}catch(e){}
function s(e){var d=JSON.stringify({p:location.pathname.toLowerCase().slice(0,80),r:r,e:e});
if(navigator.sendBeacon)navigator.sendBeacon(u,d);else fetch(u,{method:"POST",body:d,keepalive:true,mode:"no-cors"})}
s("view");document.addEventListener("click",function(ev){var a=ev.target.closest&&ev.target.closest("a");
if(a&&/github\.com\/GeRiY\/callgraph\/releases/.test(a.href))s("download")})}catch(e){}})();
</script>"""

SCRIPT_MENU = """<script>
// nyelv-menü: kívül kattintásra / Escape-re bezárul (JS nélkül is működik)
document.addEventListener('click', function (e) {
  document.querySelectorAll('details.menu[open]').forEach(function (d) { if (!d.contains(e.target)) d.open = false; });
});
document.addEventListener('keydown', function (e) {
  if (e.key !== 'Escape') return;
  document.querySelectorAll('details.menu[open]').forEach(function (d) { d.open = false; d.querySelector('summary').focus(); });
});
</script>"""

SCRIPT_TABS = """<script>
// platform-fülek (billentyűzettel is): a kiválasztott panel látszik
document.querySelectorAll('[role=tablist]').forEach(function (list) {
  var tabs = Array.prototype.slice.call(list.querySelectorAll('[role=tab]'));
  function select(t, focus) {
    tabs.forEach(function (x) {
      var on = x === t; x.setAttribute('aria-selected', on); x.tabIndex = on ? 0 : -1;
      document.getElementById(x.getAttribute('aria-controls')).hidden = !on;
    });
    if (focus) t.focus();
  }
  tabs.forEach(function (t, i) {
    t.addEventListener('click', function () { select(t, true); });
    t.addEventListener('keydown', function (e) {
      if (e.key === 'ArrowRight') select(tabs[(i + 1) % tabs.length], true);
      if (e.key === 'ArrowLeft') select(tabs[(i - 1 + tabs.length) % tabs.length], true);
    });
  });
  // JS nélkül minden panel látszik (saját címmel); JS-sel csak a kiválasztott
  var win = list.hasAttribute('data-os') && /Win/.test(navigator.platform || '');
  select(win ? tabs[tabs.length - 1] : tabs[0], false);
});
</script>"""


def script_copy(lang):
    u = UI[lang]
    return """<script>
// másoló gomb a parancs- és konfig-blokkokon (JS nélkül a blokk ugyanúgy olvasható)
if (navigator.clipboard) document.querySelectorAll('pre.code:not(.out)').forEach(function (pre) {
  var wrap = document.createElement('div'); wrap.className = 'copywrap';
  pre.parentNode.insertBefore(wrap, pre); wrap.appendChild(pre);
  var b = document.createElement('button'); b.type = 'button'; b.className = 'copy';
  b.textContent = %s; b.setAttribute('aria-label', %s); wrap.appendChild(b);
  b.addEventListener('click', function () {
    navigator.clipboard.writeText(pre.innerText.replace(/\\n$/, '')).then(function () {
      b.textContent = %s; setTimeout(function () { b.textContent = %s; }, 1600);
    });
  });
});
</script>""" % (json.dumps(u["copy"]), json.dumps(u["copy_label"]), json.dumps(u["copied"]), json.dumps(u["copy"]))


def head(page, lang, extra_ld, version):
    u = UI[lang]
    t, d = page["title"][lang], page["desc"][lang]
    canon = url(page["path"][lang])
    lines = [
        '<meta charset="utf-8">',
        '<meta name="viewport" content="width=device-width,initial-scale=1">',
        "<title>%s</title>" % esc(t),
        '<meta name="description" content="%s">' % esc(d),
    ]
    if page["key"] == "home":
        kw = {"en": "callgraph, call graph, code map, AI coding agent, Claude Code skill, MCP server, token efficient, static analysis, AST, PHP, Laravel, JavaScript, TypeScript, Vue, Rust, Swift, SwiftUI, impact analysis, who calls this",
              "hu": "callgraph, call graph, hívás-gráf, kód-térkép, AI agent, Claude Code skill, MCP-szerver, token-hatékony, statikus elemzés, AST, PHP, Laravel, JavaScript, TypeScript, Vue, Rust, Swift, SwiftUI, hatáselemzés, ki hívja"}
        lines.append('<meta name="keywords" content="%s">' % kw[lang])
    lines += [
        '<link rel="canonical" href="%s">' % canon,
        '<link rel="alternate" hreflang="en" href="%s">' % url(page["path"]["en"]),
        '<link rel="alternate" hreflang="hu" href="%s">' % url(page["path"]["hu"]),
        '<link rel="alternate" hreflang="x-default" href="%s">' % url(page["path"]["en"]),
        '<meta name="author" content="%s">' % u["author"],
        '<meta name="robots" content="index,follow">',
        '<meta name="theme-color" content="#0c152a">',
        '<link rel="icon" href="/assets/favicon.svg" type="image/svg+xml">',
        '<link rel="stylesheet" href="/assets/style.css">',
        "<script>document.documentElement.className += ' js';</script>",
        '<link rel="alternate" type="text/markdown" href="%s" title="%s">' % (md_path(page["path"][lang]), esc(u["md"])),
    ]
    if page["key"] in ("home", "changelog"):
        lines.append('<link rel="alternate" type="application/atom+xml" href="/changelog/feed.xml" title="callgraph releases">')
    lines += [
        '<meta property="og:type" content="website">',
        '<meta property="og:site_name" content="callgraph">',
        '<meta property="og:title" content="%s">' % esc(t),
        '<meta property="og:description" content="%s">' % esc(d),
        '<meta property="og:url" content="%s">' % canon,
        '<meta property="og:image" content="%s">' % OG_IMAGE,
        '<meta property="og:image:width" content="1200">',
        '<meta property="og:image:height" content="630">',
        '<meta property="og:image:alt" content="%s">' % ("callgraph terminal output: class outline with line ranges and categorized calls" if lang == "en" else "callgraph terminál-kimenet: osztály-outline sortartománnyal és kategorizált hívásokkal"),
        '<meta property="og:locale" content="%s">' % u["locale"],
        '<meta property="og:locale:alternate" content="%s">' % u["other_locale"],
        '<meta name="twitter:card" content="summary_large_image">',
        '<meta name="twitter:title" content="%s">' % esc(t),
        '<meta name="twitter:description" content="%s">' % esc(d),
        '<meta name="twitter:image" content="%s">' % OG_IMAGE,
    ]
    for obj in extra_ld + [breadcrumb_ld(page, lang)]:
        lines.append(ld(obj))
    return "\n".join(lines)


def software_ld(lang, version):
    return {"@context": "https://schema.org", "@type": "SoftwareApplication", "name": "callgraph",
            "applicationCategory": "DeveloperApplication", "operatingSystem": "macOS, Linux, Windows",
            "softwareVersion": version, "license": "https://opensource.org/licenses/MIT",
            "url": url(BY_KEY["home"]["path"][lang]), "downloadUrl": REPO + "/releases/latest",
            "codeRepository": REPO,
            "author": {"@type": "Person", "name": UI[lang]["author"], "url": "https://www.linkedin.com/in/gergomakra/",
                       "sameAs": ["https://www.linkedin.com/in/gergomakra/"]},
            "inLanguage": ["en", "hu"] if lang == "en" else ["hu", "en"],
            "description": {"en": "A token-efficient code map for AI coding agents: structure, categorized call graph, method bodies, who-calls-this and impact analysis without reading whole files, as a CLI, a Claude Code skill and a local MCP server. Supports PHP, JavaScript, TypeScript, Vue, Rust, Swift/SwiftUI, CSS/SCSS, JSON and Markdown.",
                            "hu": "Token-hatékony kód-térkép AI kódoló agenteknek: szerkezet, kategorizált hívás-gráf, metódus-törzs, ki hívja és hatáselemzés a teljes fájlok beolvasása nélkül, parancssorból, Claude Code skillként és helyi MCP-szerverként. PHP, JavaScript, TypeScript, Vue, Rust, Swift/SwiftUI, CSS/SCSS, JSON és Markdown."}[lang],
            "offers": {"@type": "Offer", "price": "0", "priceCurrency": "EUR"}}


def faq_ld(lang):
    return {"@context": "https://schema.org", "@type": "FAQPage",
            "mainEntity": [{"@type": "Question", "name": q, "acceptedAnswer": {"@type": "Answer", "text": strip_tags(a)}}
                           for q, a in FAQ[lang]]}


def faq_html(lang):
    return "\n".join("  <details><summary>%s</summary><p>%s</p></details>" % (esc(q), a) for q, a in FAQ[lang])


def howto_ld(key, lang):
    h = HOWTO[key][lang]
    return {"@context": "https://schema.org", "@type": "HowTo", "name": h["name"], "description": h["description"],
            "inLanguage": lang, "tool": [{"@type": "HowToTool", "name": "bash"}],
            "step": [{"@type": "HowToStep", "position": i + 1, "name": n, "text": t}
                     for i, (n, t) in enumerate(h["steps"])]}


def website_ld(lang):
    home = url(BY_KEY["home"]["path"][lang])
    return {"@context": "https://schema.org", "@graph": [
        {"@type": "Organization", "@id": BASE + "/#org", "name": "callgraph", "url": BASE + "/",
         "logo": BASE + "/assets/og-image.png", "sameAs": [REPO]},
        {"@type": "WebSite", "@id": home + "#website", "name": "callgraph", "url": home, "inLanguage": lang,
         "publisher": {"@id": BASE + "/#org"}}]}


def md_path(path):
    return path + "index.md"


def expand(body, lang, version):
    return fences(body).replace("{{version}}", version).replace("{{faq}}", faq_html(lang))


# --------------------------------------------------------------------------- markdown versions
class _MD(HTMLParser):
    """Turns the small HTML vocabulary of the page bodies into Markdown."""
    SKIP = {"script", "style", "button", "nav"}
    VOID = {"br", "img", "hr", "meta", "link", "input", "wbr", "source"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.out = []          # finished blocks
        self.buf = ""          # current inline text
        self.skip = 0
        self.pre = False
        self.lists = []
        self.href = []
        self.row = None
        self.rows = []
        self.cell = None
        self.head_rows = 0

    def flush(self, prefix=""):
        t = re.sub(r"[ \t\r\n]+", " ", self.buf).strip()
        if t:
            self.out.append(prefix + t)
        self.buf = ""

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if self.skip:
            self.skip += tag not in self.VOID
            return
        if tag in self.SKIP or a.get("aria-hidden") == "true":
            self.skip = 0 if tag in self.VOID else 1
            return
        if tag == "pre":
            self.flush()
            self.pre = True
            self.buf = ""
        elif self.pre:
            return
        elif tag in ("h1", "h2", "h3", "p", "summary", "div", "section", "details"):
            self.flush()
        elif tag in ("ul", "ol"):
            self.flush()
            self.lists.append(tag)
        elif tag == "li":
            self.flush()
        elif tag == "code":
            self.buf += "`"
        elif tag in ("b", "strong"):
            self.buf += "**"
        elif tag in ("i", "em"):
            self.buf += "*"
        elif tag == "a":
            self.href.append(a.get("href", ""))
            self.buf += "["
        elif tag == "br":
            self.buf += " "
        elif tag == "tr":
            self.row = []
        elif tag in ("td", "th"):
            self.cell = ""
            self._saved = self.buf
            self.buf = ""

    def handle_endtag(self, tag):
        if self.skip:
            self.skip -= 1
            return
        if tag == "pre":
            self.out.append("```\n" + self.buf.strip("\n") + "\n```")
            self.buf = ""
            self.pre = False
        elif self.pre:
            return
        elif tag == "h1":
            self.flush("# ")
        elif tag == "h2":
            self.flush("## ")
        elif tag in ("h3", "summary"):
            self.flush("### ")
        elif tag in ("p", "div", "section", "details"):
            self.flush()
        elif tag == "li":
            self.flush("- " if self.lists and self.lists[-1] == "ul" else "1. ")
        elif tag in ("ul", "ol"):
            self.flush()
            if self.lists:
                self.lists.pop()
        elif tag == "code":
            self.buf += "`"
        elif tag in ("b", "strong"):
            self.buf += "**"
        elif tag in ("i", "em"):
            self.buf += "*"
        elif tag == "a":
            h = self.href.pop() if self.href else ""
            if h.startswith("/"):
                h = BASE + h
            elif h.startswith("#"):
                h = self.page_url + h
            self.buf += "](%s)" % h
        elif tag in ("td", "th"):
            self.row.append(re.sub(r"\s+", " ", self.buf).strip().replace("|", "\\|"))
            self.buf = self._saved
        elif tag == "tr":
            self.rows.append(self.row)
            self.row = None
        elif tag == "table":
            if self.rows:
                lines = ["| " + " | ".join(self.rows[0]) + " |", "|" + "---|" * len(self.rows[0])]
                lines += ["| " + " | ".join(r) + " |" for r in self.rows[1:]]
                self.out.append("\n".join(lines))
            self.rows = []

    def handle_data(self, data):
        if self.skip:
            return
        self.buf += data


def to_markdown(page, lang, body_html):
    p = _MD()
    p.page_url = url(page["path"][lang])
    p.feed(body_html)
    p.close()
    p.flush()
    head = ["> %s" % page["desc"][lang], "",
            "Source: %s" % url(page["path"][lang]), ""]
    text = "\n\n".join(p.out)
    text = re.sub(r"\[\s*\]\([^)]*\)", "", text)
    return "\n".join(head) + "\n" + text + "\n"


def llms_txt(lang, version):
    u = UI[lang]
    if lang == "en":
        intro = ["# callgraph", "",
                 "> A token-efficient code map for AI coding agents: structure, categorized call graph, method bodies, "
                 "\"who calls this?\" and impact analysis from the AST, without reading whole files. PHP, JavaScript / "
                 "TypeScript, Vue, Rust, Swift / SwiftUI, CSS / SCSS, JSON, Markdown. One static binary (MIT), used as a "
                 "CLI, a Claude Code skill or a local MCP server (`callgraph mcp`). Latest release: v%s." % version, "",
                 "Install (macOS / Linux):", "", "```bash", INSTALL_ONELINER, "```", "",
                 "MCP server in Claude Code (macOS / Linux):", "", "```bash", MCP_ADD_UNIX, "```", "",
                 "## Pages", ""]
    else:
        intro = ["# callgraph", "",
                 "> Token-hatékony kód-térkép AI kódoló agenteknek: szerkezet, kategorizált hívás-gráf, metódus-törzs, "
                 "„ki hívja?” és hatáselemzés az AST-ből, a teljes fájlok beolvasása nélkül. PHP, JavaScript / TypeScript, "
                 "Vue, Rust, Swift / SwiftUI, CSS / SCSS, JSON, Markdown. Egyetlen statikus bináris (MIT): parancssorból, "
                 "Claude Code skillként vagy helyi MCP-szerverként (`callgraph mcp`). Legújabb kiadás: v%s." % version, "",
                 "Telepítés (macOS / Linux):", "", "```bash", INSTALL_ONELINER, "```", "",
                 "MCP-szerver a Claude Code-ban (macOS / Linux):", "", "```bash", MCP_ADD_UNIX, "```", "",
                 "## Oldalak", ""]
    pages = ["- [%s](%s): %s" % (p["crumb"][lang] if p["key"] != "home" else u["home"], url(md_path(p["path"][lang])),
                                 p["desc"][lang]) for p in PAGES]
    tail = ["", "## %s" % ("Optional" if lang == "en" else "További"), "",
            "- [GitHub repository](%s)" % REPO,
            "- [Releases](%s/releases)" % REPO,
            "- [Atom feed](%s/changelog/feed.xml)" % BASE]
    if lang == "en":
        tail.append("- [Hungarian version](%s/hu/llms.txt)" % BASE)
    else:
        tail.append("- [English version](%s/llms.txt)" % BASE)
    return "\n".join(intro + pages + tail) + "\n"


def atom_feed(releases):
    def xesc(s):
        return html.escape(s, quote=True)
    updated = max(r["date"] for r in releases if r["date"]) + "T00:00:00Z"
    out = ['<?xml version="1.0" encoding="utf-8"?>', '<feed xmlns="http://www.w3.org/2005/Atom" xml:lang="en">',
           "  <title>callgraph releases</title>",
           "  <subtitle>What was added, changed and fixed in each callgraph release.</subtitle>",
           '  <link rel="self" type="application/atom+xml" href="%s/changelog/feed.xml"/>' % BASE,
           '  <link rel="alternate" type="text/html" href="%s/changelog/"/>' % BASE,
           "  <id>%s/changelog/</id>" % BASE, "  <updated>%s</updated>" % updated,
           "  <author><name>Gergő Makra</name></author>",
           '  <icon>%s/assets/favicon.svg</icon>' % BASE]
    for r in releases:
        parts = []
        for b in r["blocks"]:
            if b["title"]:
                parts.append("<h3>%s</h3>" % esc(b["title"]))
            parts += ["<p>%s</p>" % md_inline(p) for p in b["paras"]]
            if b["items"]:
                parts.append("<ul>%s</ul>" % "".join("<li>%s</li>" % md_inline(i) for i in b["items"]))
        d = (r["date"] or updated[:10]) + "T00:00:00Z"
        out += ["  <entry>", "    <title>callgraph v%s</title>" % xesc(r["version"]),
                '    <link rel="alternate" type="text/html" href="%s/changelog/#v%s"/>' % (BASE, xesc(r["version"])),
                '    <link rel="related" type="text/html" href="%s"/>' % xesc(r["url"]),
                "    <id>%s</id>" % xesc(r["url"]), "    <updated>%s</updated>" % d, "    <published>%s</published>" % d,
                '    <content type="html">%s</content>' % xesc("".join(parts)), "  </entry>"]
    out.append("</feed>")
    return "\n".join(out) + "\n"


def render(page, lang, body, version):
    extra = []
    if page["key"] == "home":
        extra = [website_ld(lang), software_ld(lang, version), faq_ld(lang), howto_ld("install", lang)]
    elif page["key"] == "mcp":
        extra = [howto_ld("mcp", lang)]
    body = expand(body, lang, version)
    scripts = SCRIPT_MENU + "\n" + SCRIPT_COUNT + "\n" + script_copy(lang) + ("\n" + SCRIPT_TABS if 'role="tablist"' in body else "")
    return ('<!DOCTYPE html>\n<html lang="%s">\n<head>\n%s\n</head>\n<body>\n<a class="skip" href="#main">%s</a>\n'
            '<div class="wrap">\n%s\n<main id="main">\n%s%s</main>\n\n%s\n</div>\n%s\n</body>\n</html>\n'
            % (lang, head(page, lang, extra, version), UI[lang]["skip"], nav(page, lang),
               breadcrumb_html(page, lang) + "\n" if page["key"] != "home" else "", body,
               footer(page, lang), scripts))


def render_404(version):
    links = "".join('<a class="btn" href="%s">%s</a>' % (BY_KEY[k]["path"]["en"], l) for k, l in
                    [("mcp", "MCP setup"), ("php", "PHP"), ("rust", "Rust"), ("typescript", "TypeScript / Vue"),
                     ("swift", "Swift"), ("locate", "locate"), ("profile", "Project profile"), ("changelog", "Changelog")])
    body = ('<section class="hero page" aria-labelledby="title"><div>\n'
            '<h1 id="title">Page not found</h1>\n'
            '<p class="lead">This page does not exist. · <span lang="hu">Ez az oldal nem létezik.</span></p>\n'
            '<div class="cta"><a class="btn primary" href="/">Home</a>'
            '<a class="btn" href="/hu/" lang="hu" hreflang="hu">Kezdőlap (magyar)</a>'
            '<a class="btn" href="%s">GitHub</a></div>\n</div></section>\n'
            '<section aria-labelledby="h-pages"><h2 id="h-pages">Main pages</h2>\n'
            '<p class="sub"><span lang="hu">Minden oldal magyarul is elérhető a <a href="/hu/">/hu/</a> alatt.</span></p>\n'
            '<div class="cta">%s</div></section>\n' % (REPO, links))
    return ('<!DOCTYPE html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n'
            '<meta name="viewport" content="width=device-width,initial-scale=1">\n'
            '<title>Page not found — callgraph</title>\n<meta name="robots" content="noindex">\n'
            '<link rel="icon" href="/assets/favicon.svg" type="image/svg+xml">\n'
            '<link rel="stylesheet" href="/assets/style.css">\n</head>\n<body>\n'
            '<a class="skip" href="#main">Skip to content</a>\n<div class="wrap">\n%s\n<main id="main">\n%s</main>\n\n%s\n</div>\n%s\n</body>\n</html>\n'
            % (nav(None, "en"), body, footer(None, "en"), SCRIPT_MENU))


def out_file(path):
    rel = path.strip("/")
    return os.path.join(ROOT, rel, "index.html") if rel else os.path.join(ROOT, "index.html")


def sitemap(lastmod):
    out = ['<?xml version="1.0" encoding="UTF-8"?>',
           '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">']
    for page in PAGES:
        for lang in LANGS:
            pr = page["priority"] if lang == "en" else "%.1f" % max(0.1, float(page["priority"]) - 0.1)
            out += ["  <url>", "    <loc>%s</loc>" % url(page["path"][lang])]
            for l2 in LANGS:
                out.append('    <xhtml:link rel="alternate" hreflang="%s" href="%s"/>' % (l2, url(page["path"][l2])))
            out.append('    <xhtml:link rel="alternate" hreflang="x-default" href="%s"/>' % url(page["path"]["en"]))
            out += ["    <lastmod>%s</lastmod>" % lastmod, "    <priority>%s</priority>" % pr, "  </url>"]
    out.append("</urlset>")
    return "\n".join(out) + "\n"


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def user_facing(releases):
    """Only the entries a user cares about (see DEV_ENTRY); empty sections are dropped."""
    for release in releases:
        for block in release["blocks"]:
            block["items"] = [i for i in block["items"] if not DEV_ENTRY.search(i)]
            block["paras"] = [p for p in block["paras"] if not DEV_ENTRY.search(p)]
        release["blocks"] = [b for b in release["blocks"] if b["items"] or b["paras"]]
    return releases


def main():
    if ROOT is None or not os.path.isdir(ROOT):
        sys.exit("usage: python3 site/build.py <gh-pages checkout>")
    releases = user_facing(parse_changelog(CHANGELOG))
    if not releases:
        sys.exit("no releases found in CHANGELOG.md")
    releases_hu = user_facing(parse_changelog(CHANGELOG_HU))
    versions = [r["version"] for r in releases]
    versions_hu = [r["version"] for r in releases_hu]
    if versions != versions_hu:
        sys.exit("CHANGELOG.hu.md is out of step with CHANGELOG.md: %s vs %s" % (versions_hu, versions))
    version = releases[0]["version"]
    lastmod = os.environ.get("LASTMOD") or max(r["date"] for r in releases if r["date"])
    written = []
    for page in PAGES:
        for lang in LANGS:
            if page["key"] == "changelog":
                body = changelog_body(lang, releases if lang == "en" else releases_hu)
            else:
                with open(os.path.join(SITE_DIR, lang, page["key"] + ".html"), encoding="utf-8") as f:
                    body = f.read()
            dest = out_file(page["path"][lang])
            write(dest, render(page, lang, body, version))
            md = to_markdown(page, lang, expand(body, lang, version))
            write(os.path.join(os.path.dirname(dest), "index.md"), md)
            written.append(os.path.relpath(dest, ROOT))
    write(os.path.join(ROOT, "404.html"), render_404(version))
    write(os.path.join(ROOT, "sitemap.xml"), sitemap(lastmod))
    write(os.path.join(ROOT, "llms.txt"), llms_txt("en", version))
    write(os.path.join(ROOT, "hu", "llms.txt"), llms_txt("hu", version))
    write(os.path.join(ROOT, "changelog", "feed.xml"), atom_feed(releases))
    print("built %d pages (+ index.md each) + 404.html + sitemap.xml + llms.txt + changelog/feed.xml "
          "(version %s, lastmod %s)" % (len(written), version, lastmod))
    for w in written:
        print("  " + w)


if __name__ == "__main__":
    main()
