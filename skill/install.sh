#!/usr/bin/env bash
# callgraph skill telepítő: ellenőrzi a callgraph platform-binárist, létrehozza az üres projects.json-t.
# Az elemzést és a projekt- / feedback-kezelést a natív callgraph bináris végzi (bin/<os>-<arch>/callgraph) — PHP, Node és jq nem kell.
# Az üzenetek nyelve: CALLGRAPH_LANG › a skill-mappa lang fájlja (friss telepítésnél még nincs) › angol.
# Használat:  bash install.sh [--hooks|--no-hooks] [--session-hook|--no-session-hook] [--mcp|--no-mcp]
#   --hooks: a Claude Code-hook (háttér-frissítés fájl-írás után) kérdés nélkül; --no-hooks: kérdés nélkül kihagyja.
#   --session-hook: a SessionStart hook (ajánlott beállítások, frissítés-értesítés) kérdés nélkül; --no-session-hook: kihagyja.
#   --rules: a callgraph-first szabály (.claude/rules/callgraph.md) kérdés nélkül (nem interaktívan ez az alap); --no-rules: kihagyja.
#   --nudge-hook: a PreToolUse tipp-hook (Grep / Read / Bash-grep előtt nem blokkoló `locate`-tipp) kérdés nélkül; --no-nudge-hook: kihagyja
#     (alapból KI, interaktívan alapértelmezett nem); --remove-nudge-hook: csak ezt a hookot veszi ki a settings.json-ból.
#   --mcp: az MCP-szerver (`callgraph mcp`) regisztrálása a Claude Code-ban kérdés nélkül; --no-mcp: kihagyja.
set -euo pipefail
HOOKS=""
SESSION_HOOK=""
MCP=""
RULES=""
NUDGE=""
for arg in "$@"; do
    case "$arg" in
        --hooks) HOOKS=yes ;;
        --no-hooks) HOOKS=no ;;
        --session-hook) SESSION_HOOK=yes ;;
        --no-session-hook) SESSION_HOOK=no ;;
        --rules) RULES=yes ;;
        --no-rules) RULES=no ;;
        --nudge-hook) NUDGE=yes ;;
        --no-nudge-hook) NUDGE=no ;;
        --remove-nudge-hook) NUDGE=remove ;;
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
    # SessionStart hook: a session elején szól, ha egy ajánlott beállítás ki van kapcsolva, vagy újabb verzió van
    # (csak kérdez; bármit csak a felhasználó igenje után állít be). Interaktívan rákérdez (alapból igen), nem
    # interaktívan csak a --session-hook kapcsolóval.
    want_session="$SESSION_HOOK"
    if [[ -z "$want_session" && -t 0 && -t 1 ]]; then
        if [[ "$(ui_lang)" == hu ]]; then
            printf 'Claude Code-hook a session elején: szóljon, ha egy ajánlott callgraph-beállítás ki van kapcsolva vagy újabb verzió érhető el (csak kérdez, semmit nem állít be magától)? [I/n] '
        else
            printf 'Claude Code hook at session start: tell you when a recommended callgraph setting is off or a newer version is out (it only asks, never changes anything by itself)? [Y/n] '
        fi
        read -r answer || answer=""
        case "$answer" in [nN]*) want_session=no ;; *) want_session=yes ;; esac
    fi
    if [[ "$want_session" == yes ]]; then
        "./$RPG" hook install-session || msg "⚠ The session hook was not installed (see above)." "⚠ A session-hook nem települt (lásd fent)."
    else
        msg "Session hook skipped — add later: bash callgraph.sh hook install-session" \
            "Session-hook kihagyva — később: bash callgraph.sh hook install-session"
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
# Callgraph-first szabály: egy mondat a Claude Code utasítás-csatornájában (.claude/rules/callgraph.md) — enélkül a modell
# a Grep / Read szokásához ragaszkodik, a skill leírását nem követi. Projekt-telepítés (<projekt>/.claude/skills/callgraph)
# → <projekt>/.claude/rules/callgraph.md, a szabályban a projekt-relatív úttal; felhasználói telepítés (~/.claude/skills/
# callgraph) → ~/.claude/rules/callgraph.md, a szabályban a ~/.claude/… úttal. Idempotens: azonos tartalomhoz nem nyúl;
# eltérő meglévő fájlról előbb callgraph.md.bak készül. Máshová telepített skillnél kihagyja (az út nem volna érvényes).
install_rule() {
    local template="$DIR/extras/rules/callgraph.md" claude_dir skills_dir target rule_path tmp
    [[ -f "$template" ]] || return 0
    skills_dir="$(cd "$DIR/.." && pwd)"
    claude_dir="$(cd "$DIR/../.." && pwd)"
    if [[ "$(basename "$DIR")" != callgraph || "$(basename "$skills_dir")" != skills || "$(basename "$claude_dir")" != .claude ]]; then
        msg "Rule skipped: the skill is not in a .claude/skills/callgraph folder — add the rule by hand: $template" \
            "Szabály kihagyva: a skill nem egy .claude/skills/callgraph mappában van — add hozzá kézzel: $template"
        return 0
    fi
    target="$claude_dir/rules/callgraph.md"
    rule_path=".claude/skills/callgraph/callgraph.sh"
    if [[ "$claude_dir" == "$HOME/.claude" ]]; then
        rule_path="~/.claude/skills/callgraph/callgraph.sh"
    fi
    tmp="$(mktemp)"
    sed "s#\.claude/skills/callgraph/callgraph\.sh#$rule_path#" "$template" > "$tmp"
    if [[ -f "$target" ]] && cmp -s "$tmp" "$target"; then
        rm -f "$tmp"
        msg "✓ Rule already in place: $target" "✓ A szabály már a helyén van: $target"
        return 0
    fi
    mkdir -p "$claude_dir/rules"
    if [[ -f "$target" ]]; then
        cp "$target" "$target.bak"
        msg "  The existing $target differs — saved as $target.bak" \
            "  A meglévő $target eltér — mentve: $target.bak"
    fi
    cp "$tmp" "$target"
    rm -f "$tmp"
    msg "✓ Rule written: $target (remove the file to turn it off)" \
        "✓ Szabály kiírva: $target (kikapcsolás: töröld a fájlt)"
}
want_rules="$RULES"
if [[ -z "$want_rules" && -t 0 && -t 1 ]]; then
    if [[ "$(ui_lang)" == hu ]]; then
        printf 'Egy mondatos szabály a Claude Code-nak (.claude/rules/callgraph.md): kódkereséshez a callgraph, ne Grep / Read? [I/n] '
    else
        printf 'Write a one-sentence rule for Claude Code (.claude/rules/callgraph.md): use callgraph, not Grep / Read, to navigate code? [Y/n] '
    fi
    read -r answer || answer=""
    case "$answer" in [nN]*) want_rules=no ;; *) want_rules=yes ;; esac
fi
if [[ "$want_rules" == no ]]; then
    msg "Rule skipped — add later: bash install.sh --rules" "Szabály kihagyva — később: bash install.sh --rules"
else
    install_rule
fi
# Opt-in PreToolUse tipp-hook (`callgraph hook pre-nudge`): Grep / Read / Bash-grep előtt egy nem blokkoló sor a
# `locate`-ról (additionalContext). A settings.json-t a bináris szerkeszti (a többi beállítás és hook marad, előtte
# settings.json.bak, nincs duplikátum — nem kell hozzá python / jq). Projekt-telepítés → <projekt>/.claude/settings.json,
# felhasználói telepítés → ~/.claude/settings.json (mint a szabálynál). Alapból KI; interaktívan alapértelmezett nem.
nudge_settings() {
    local skills_dir claude_dir
    skills_dir="$(cd "$DIR/.." && pwd)"
    claude_dir="$(cd "$DIR/../.." && pwd)"
    if [[ "$(basename "$DIR")" == callgraph && "$(basename "$skills_dir")" == skills && "$(basename "$claude_dir")" == .claude ]]; then
        echo "$claude_dir/settings.json"
    else
        echo "$HOME/.claude/settings.json"
    fi
}
if [[ -n "$RPG" ]]; then
    want_nudge="$NUDGE"
    if [[ -z "$want_nudge" && -t 0 && -t 1 ]]; then
        if [[ "$(ui_lang)" == hu ]]; then
            printf 'Opcionális Claude Code-hook: Grep / Read előtt rövid, nem blokkoló tipp a callgraph locate-ra (settings.json)? [i/N] '
        else
            printf 'Optional Claude Code hook: a short, non-blocking hint toward callgraph locate before Grep / Read (settings.json)? [y/N] '
        fi
        read -r answer || answer=""
        case "$answer" in [yYiI]*) want_nudge=yes ;; *) want_nudge=no ;; esac
    fi
    nudge_file="$(nudge_settings)"
    case "$want_nudge" in
        yes)
            "./$RPG" hook install-nudge --settings "$nudge_file" || msg "⚠ The nudge hook was not installed (see above)." "⚠ A tipp-hook nem települt (lásd fent)." ;;
        remove)
            "./$RPG" hook uninstall-nudge --settings "$nudge_file" || msg "⚠ The nudge hook was not removed (see above)." "⚠ A tipp-hook nem lett eltávolítva (lásd fent)." ;;
        *)
            msg "Nudge hook skipped — add later: bash install.sh --nudge-hook (remove: bash install.sh --remove-nudge-hook)" \
                "Tipp-hook kihagyva — később: bash install.sh --nudge-hook (eltávolítás: bash install.sh --remove-nudge-hook)" ;;
    esac
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
msg "Other agents (Cursor, Codex CLI, VS Code, Windsurf): $PWD/${RPG:-bin/<os>-<arch>/callgraph} setup --dry-run  (then without --dry-run)" \
    "Más ágensek (Cursor, Codex CLI, VS Code, Windsurf): $PWD/${RPG:-bin/<os>-<arch>/callgraph} setup --dry-run  (utána --dry-run nélkül)"
msg "Restart Claude Code to load the skill (and the MCP server)." \
    "Indítsd újra a Claude Code-ot, hogy betöltse a skillt (és az MCP-szervert)."
