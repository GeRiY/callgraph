# callgraph

Token-hatékony kód-térkép agenteknek (Claude Code skill): PHP, JS / TS / Vue, CSS / SCSS, JSON és Markdown fájlok
szerkezete, kategorizált hívás-gráfja, metódus-törzse, „ki hívja” keresése és hatáselemzése — a fájl beolvasása nélkül.
Az elemző egy önálló Rust-bináris (`callgraph`); a `skill/callgraph.sh` vékony burok fölötte.

- Használat és telepítés: [`skill/SKILL.md`](skill/SKILL.md), [`skill/README.md`](skill/README.md)
- Felépítés: [`ARCHITECTURE.md`](ARCHITECTURE.md), a lekérdező réteg: [`docs/QUERY.md`](docs/QUERY.md), új nyelv: [`docs/UJ-NYELV.md`](docs/UJ-NYELV.md)

## Letöltés és telepítés

A kész, telepíthető csomag (skill-fájlok, az adott gép binárisa, útmutató) a legfrissebb
[GitHub Release](https://github.com/GeRiY/callgraph/releases/latest)-ből tölthető le:

| Gép | Letöltés |
|---|---|
| Mac, Apple Silicon (M1–M4) | [callgraph-darwin-arm64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-arm64.zip) |
| Mac, Intel | [callgraph-darwin-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-darwin-x86_64.zip) |
| Linux x86_64 | [callgraph-linux-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-linux-x86_64.zip) |
| Linux ARM64 | [callgraph-linux-aarch64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-linux-aarch64.zip) |
| Windows x86_64 (Git Bash) | [callgraph-windows-x86_64.zip](https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-windows-x86_64.zip) |

Mind az öt bináris egy csomagban: a Release `callgraph-<verzió>.tar.gz` fájlja.

**Gyors telepítés (macOS / Linux).** Letölti a géphez illő csomagot a `~/.claude/skills/callgraph/` mappába, és
lefuttatja az `install.sh`-t. Frissítésnél a `projects.json`, az `errors.jsonl` és a `feedback.jsonl` megmarad.

```bash
p="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)" && t="$(mktemp -d)" \
  && curl -fsSL -o "$t/cg.zip" "https://github.com/GeRiY/callgraph/releases/latest/download/callgraph-$p.zip" \
  && mkdir -p ~/.claude/skills && unzip -qo "$t/cg.zip" -d ~/.claude/skills \
  && bash ~/.claude/skills/callgraph/install.sh && rm -rf "$t"
```

**Kézzel (Windows-on is):** csomagold ki a zipet, a benne lévő `callgraph/` mappát másold a `~/.claude/skills/` alá
(Windows: `%USERPROFILE%\.claude\skills\`), majd futtasd ott a `bash install.sh`-t. Részletek: [`skill/TELEPITES.md`](skill/TELEPITES.md).

Utána regisztráld a projektet (`bash ~/.claude/skills/callgraph/projects.sh add myapp /abszolút/út/a/projekthez`),
és indítsd újra a Claude Code-ot.

## Build

```bash
cargo build --release          # target/release/callgraph
cargo test --release
bash tools/build-skill.sh      # dist/callgraph-<verzió>.tar.gz (a skill-csomag)
```

A több platformos csomagot (macOS arm64 / x86_64, Linux arm64 / x86_64 musl, Windows x86_64) a GitHub Actions
készíti (`.github/workflows/release.yml`); `v*` tagnél GitHub Release-be teszi.

## Licenc

MIT — lásd [`LICENSE`](LICENSE).
