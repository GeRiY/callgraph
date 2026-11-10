#!/usr/bin/env bash
# callgraph skill telepítő: ellenőrzi a callgraph platform-binárist, a SKILL.md-t az aktuális nyelvűre állítja
# (skill-i18n/SKILL.<en|hu>.md), létrehozza az üres projects.json-t.
# Az elemzést és a projekt- / feedback-kezelést a natív callgraph bináris végzi (bin/<os>-<arch>/callgraph) — PHP, Node és jq nem kell.
# Az üzenetek nyelve: CALLGRAPH_LANG › a skill-mappa lang fájlja (friss telepítésnél még nincs) › angol.
# Használat:  bash install.sh [--hooks|--no-hooks] [--mcp|--no-mcp]
#   --hooks: a Claude Code-hook (háttér-frissítés fájl-írás után) kérdés nélkül; --no-hooks: kérdés nélkül kihagyja.
#   --mcp: az MCP-szerver (`callgraph mcp`) regisztrálása a Claude Code-ban kérdés nélkül; --no-mcp: kihagyja.
set -euo pipefail
HOOKS=""
MCP=""
for arg in "$@"; do
    case "$arg" in
        --hooks) HOOKS=yes ;;
        --no-hooks) HOOKS=no ;;
        --mcp) MCP=yes ;;
        --no-mcp) MCP=no ;;
    esac
done
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$DIR"
source "$DIR/lib.sh"

os="$(uname -s | tr '[:upper:]' '[:lower:]')"; arch="$(uname -m)"
case "$os" in mingw*|msys*|cygwin*) os=windows ;; esac   # Git Bash: MINGW64_NT-… → windows
case "$arch" in amd64|AMD64) arch=x86_64 ;; esac
# macOS: a böngészőből letöltött (aláíratlan) bináris karantén-jelzőjét a Gatekeeper blokkolja — levesszük a bin/-ről.
if [[ "$os" == darwin ]] && command -v xattr >/dev/null; then
    xattr -dr com.apple.quarantine bin 2>/dev/null || true
fi
chmod +x ./*.sh bin/*/callgraph* 2>/dev/null || true
RPG=""
for c in "bin/$os-$arch/callgraph" "bin/$os-$arch/callgraph.exe" "bin/callgraph" "bin/callgraph.exe"; do
    [[ -x "$c" ]] && { RPG="$c"; break; }
done
if [[ -n "$RPG" ]]; then
    msg "✓ callgraph binary: $RPG" "✓ callgraph bináris: $RPG"
else
    msg "⚠ No callgraph binary for this platform (bin/$os-$arch/callgraph)." \
        "⚠ Nincs callgraph bináris ehhez a platformhoz (bin/$os-$arch/callgraph)."
    msg "  Build it (cargo build --release) and copy it there, or set: CALLGRAPH_BIN=/path/callgraph" \
        "  Építsd meg (cargo build --release) és másold oda, vagy add meg: CALLGRAPH_BIN=/út/callgraph"
fi
# a skill-utasítás (SKILL.md) az aktuális nyelven, a skill-i18n/ forrásból (nyelvváltáskor a set-lang cseréli)
if [[ -n "$RPG" ]]; then
    CALLGRAPH_SKILL_DIR="$DIR" PHP_AST_HOME="${PHP_AST_HOME:-$DIR}" "./$RPG" skill-sync || true
fi
[[ -f projects.json ]] || { echo '{}' > projects.json; msg "✓ empty projects.json" "✓ üres projects.json"; }
# Claude Code-hook (PostToolUse: Edit / Write / MultiEdit → `callgraph refresh` a háttérben): interaktívan rákérdez
# (alapból nem), nem interaktívan csak a --hooks kapcsolóval; a ~/.claude/settings.json szerkesztését a bináris végzi
# (a többi beállítás marad, előtte settings.json.bak). Eltávolítás: callgraph.sh hook uninstall
if [[ -n "$RPG" ]]; then
    want_hooks="$HOOKS"
    if [[ -z "$want_hooks" && -t 0 && -t 1 ]]; then
        if [[ "$(ui_lang)" == hu ]]; then
            printf 'Claude Code-hook: a szerkesztett fájlok a háttérben frissüljenek a callgraph-ban (~/.claude/settings.json)? [i/N] '
        else
            printf 'Claude Code hook: refresh edited files in the callgraph in the background (~/.claude/settings.json)? [y/N] '
        fi
        read -r answer || answer=""
        case "$answer" in [yYiI]*) want_hooks=yes ;; *) want_hooks=no ;; esac
    fi
    if [[ "$want_hooks" == yes ]]; then
        "./$RPG" hook install || msg "⚠ The hook was not installed (see above)." "⚠ A hook nem települt (lásd fent)."
    else
        msg "Hook skipped — add later: bash callgraph.sh hook install (remove: bash callgraph.sh hook uninstall)" \
            "Hook kihagyva — később: bash callgraph.sh hook install (eltávolítás: bash callgraph.sh hook uninstall)"
    fi
fi
# MCP-szerver a Claude Code-ban (`claude mcp add -s user callgraph -- <bináris> mcp`): ha a `claude` CLI elérhető,
# interaktívan rákérdez (alapból nem), nem interaktívan csak a --mcp kapcsolóval. Idempotens: ha már ezzel az úttal
# regisztrált, nem nyúl hozzá; más úttal regisztrált → eltávolítja és újra felveszi. Hiba esetén csak figyelmeztet.
MCP_DOCS="https://github.com/GeRiY/callgraph/blob/main/docs/MCP.md"
register_mcp() {
    local bin="$1"
    if claude mcp get callgraph 2>/dev/null | grep -qF -- "$bin"; then
        msg "✓ MCP server already registered for Claude Code: callgraph" \
            "✓ Az MCP-szerver már regisztrálva van a Claude Code-ban: callgraph"
        return 0
    fi
    claude mcp remove -s user callgraph >/dev/null 2>&1 || true
    claude mcp add -s user callgraph -- "$bin" mcp >/dev/null 2>&1 || return 1
    msg "✓ MCP server registered for Claude Code (user scope): callgraph → $bin mcp" \
        "✓ MCP-szerver regisztrálva a Claude Code-ban (user scope): callgraph → $bin mcp"
}
if [[ -n "$RPG" ]]; then
    mcp_bin="$DIR/$RPG"
    if [[ "$os" == windows ]] && command -v cygpath >/dev/null 2>&1; then
        mcp_bin="$(cygpath -w "$mcp_bin")"   # Git Bash: a claude CLI Windows-utat vár (C:\Users\…)
    fi
    if command -v claude >/dev/null 2>&1; then
        want_mcp="$MCP"
        if [[ -z "$want_mcp" && -t 0 && -t 1 ]]; then
            if [[ "$(ui_lang)" == hu ]]; then
                printf 'Regisztráljam a callgraph-ot MCP-szerverként a Claude Code-ban (claude mcp add -s user)? [i/N] '
            else
                printf 'Register callgraph as an MCP server for Claude Code (claude mcp add -s user)? [y/N] '
            fi
            read -r answer || answer=""
            case "$answer" in [yYiI]*) want_mcp=yes ;; *) want_mcp=no ;; esac
        fi
        if [[ "$want_mcp" == yes ]]; then
            if register_mcp "$mcp_bin"; then
                msg "  Remove: claude mcp remove -s user callgraph" \
                    "  Eltávolítás: claude mcp remove -s user callgraph"
            else
                msg "⚠ The MCP server was not registered — by hand: claude mcp add -s user callgraph -- \"$mcp_bin\" mcp" \
                    "⚠ Az MCP-szerver nem regisztrálódott — kézzel: claude mcp add -s user callgraph -- \"$mcp_bin\" mcp"
            fi
        else
            msg "MCP server skipped — add later: claude mcp add -s user callgraph -- \"$mcp_bin\" mcp" \
                "MCP-szerver kihagyva — később: claude mcp add -s user callgraph -- \"$mcp_bin\" mcp"
        fi
    elif [[ "$MCP" == yes ]]; then
        msg "⚠ --mcp: the claude CLI is not on PATH — register by hand: claude mcp add -s user callgraph -- \"$mcp_bin\" mcp" \
            "⚠ --mcp: a claude CLI nincs a PATH-on — kézzel: claude mcp add -s user callgraph -- \"$mcp_bin\" mcp"
    fi
    msg "MCP for Claude Desktop / Cursor / VS Code: $MCP_DOCS (binary: $mcp_bin)" \
        "MCP Claude Desktophoz / Cursorhoz / VS Code-hoz: $MCP_DOCS (bináris: $mcp_bin)"
fi
# a helyi statisztika beküldése (opt-in): csak interaktív terminálban és még döntés nélkül kérdez (Igen / Most nem /
# Soha); a döntést és a terminál-vizsgálatot a bináris végzi (nem interaktívan, CI-ban semmit nem tesz)
if [[ -n "$RPG" ]]; then
    CALLGRAPH_SKILL_DIR="$DIR" PHP_AST_HOME="${PHP_AST_HOME:-$DIR}" "./$RPG" telemetry ask || true
fi
msg "Done. Register a project: bash projects.sh add <alias> <absolute_path>" \
    "Kész. Projekt regisztrálása: bash projects.sh add <alias> <abszolút_út>"
msg "Language: $(ui_lang) — switch: bash callgraph.sh set-lang <en|hu>" \
    "Nyelv: $(ui_lang) — váltás: bash callgraph.sh set-lang <en|hu>"
msg "Restart Claude Code to load the skill (and the MCP server)." \
    "Indítsd újra a Claude Code-ot, hogy betöltse a skillt (és az MCP-szervert)."
