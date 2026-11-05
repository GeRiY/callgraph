# callgraph — telepítés

1. Csomagold ki a zipet; a benne lévő `callgraph/` mappát másold a Claude Code skill-mappájába:
   `~/.claude/skills/callgraph/` (Windows: `%USERPROFILE%\.claude\skills\callgraph\`).
   Frissítésnél a meglévő mappa fölé másold: a `projects.json`, az `errors.jsonl`, a `feedback.jsonl` és a `lang` (nyelvi beállítás) megmarad.
2. Futtasd a skill-mappában: `bash install.sh` — futtathatóvá teszi a scripteket és a binárist, macOS-en leveszi
   a letöltés karantén-jelzőjét, és ellenőrzi, hogy van-e a gépedhez illő bináris (`bin/<os>-<arch>/callgraph`).
3. Regisztrálj egy projektet: `bash ~/.claude/skills/callgraph/projects.sh add myapp /abszolút/út/a/projekthez`
4. Próba: `bash ~/.claude/skills/callgraph/callgraph.sh --project myapp app/Models/User.php`
5. Indítsd újra a Claude Code-ot, hogy a skillt (`SKILL.md`) betöltse.
6. Nyelv (nem kötelező): a kimenet és a súgó alapból angol; magyarra: `bash ~/.claude/skills/callgraph/callgraph.sh lang hu`
   (vissza: `lang en`). Egy futásra a `CALLGRAPH_LANG=hu` környezeti változó is jó.

Melyik csomag kell?

| Gép | Csomag |
|---|---|
| Mac, Apple Silicon (M1–M4) | `callgraph-darwin-arm64` |
| Mac, Intel | `callgraph-darwin-x86_64` |
| Linux x86_64 | `callgraph-linux-x86_64` |
| Linux ARM64 | `callgraph-linux-aarch64` |
| Windows (Git Bash) | `callgraph-windows-x86_64` |

A teljes csomag (`callgraph-package`, `callgraph-<verzió>.tar.gz`) mind az öt binárist tartalmazza.
Függőség: csak `bash` (PHP, Node, jq nem kell). Részletek: `README.md`.
