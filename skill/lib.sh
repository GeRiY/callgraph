#!/usr/bin/env bash
# lib.sh — a callgraph skill bash-indítóinak közös része (callgraph.sh, projects.sh, feedback.sh).
# Nem önállóan futtatandó — source-old be: `source "$SCRIPT_DIR/lib.sh"`
#
# A logika (projekt-aliasok, feedback, hibanapló error-id-val) a callgraph binárisban él:
#   callgraph wrap | project | feedback …
# Az indítók csak a súgót írják ki és továbbítanak; a régi scripteknek a `fail` / `abs_path` / `resolve_project_*` is itt van. jq nem kell.

# LIB_DIR: a skill fizikai helye — itt él a projects.json, az errors.jsonl és a feedback.jsonl.
LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# PHP_AST_HOME: a hibaüzenetekben / súgóban MEGJELENÍTETT skill-út (felülbírálható, pl. symlinkelt telepítésnél).
PHP_AST_HOME="${PHP_AST_HOME:-$LIB_DIR}"

# Egy nyelvi érték normalizálása, mint a binárisban (Lang::parse): trim, a "_" / "-" / "." előtti rész, kisbetűsen;
# "en" / "hu" → kiírja, minden más → semmit (nem érvényes).
_lang_code() {
    local v="$1"
    v="${v#"${v%%[![:space:]]*}"}"; v="${v%"${v##*[![:space:]]}"}"
    v="${v%%[_.-]*}"
    v="$(printf '%s' "$v" | tr '[:upper:]' '[:lower:]')"
    case "$v" in en|hu) printf '%s\n' "$v" ;; esac
}

# A terminál-szövegek nyelve (en / hu), ugyanúgy, mint a binárisban: CALLGRAPH_LANG › <skill-mappa>/lang › en.
ui_lang() {
    local code=""
    [[ -n "${CALLGRAPH_LANG:-}" ]] && code="$(_lang_code "$CALLGRAPH_LANG")"
    if [[ -z "$code" && -r "$LIB_DIR/lang" ]]; then
        code="$(_lang_code "$(cat "$LIB_DIR/lang" 2>/dev/null)")"
    fi
    printf '%s\n' "${code:-en}"
}

# Egyszeri kétnyelvű üzenet: msg "english" "magyar" → az aktuális nyelvű sor (stdout).
msg() {
    if [[ "$(ui_lang)" == hu ]]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi
}

# A súgó: <script-mappa>/help/<script>.<nyelv>.txt (tartalék: .en.txt, végül a script fejléc-kommentje),
# a ~/.claude/skills/callgraph út helyén a megjelenített skill-úttal.
print_help() {
    local script="$1" name dir f=""
    name="$(basename "$script" .sh)"; dir="$(cd "$(dirname "$script")" && pwd)"
    for f in "$dir/help/$name.$(ui_lang).txt" "$dir/help/$name.en.txt" ""; do
        [[ -z "$f" || -r "$f" ]] && break
    done
    if [[ -n "$f" ]]; then
        cat "$f"
    else
        awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "$script"
    fi | sed "s|~/.claude/skills/callgraph|$PHP_AST_HOME|g"
}

# A callgraph bináris: env-ből, vagy a skill bin/ mappájából platform szerint (bin/<os>-<arch>/callgraph), vagy bin/callgraph.
callgraph_bin() {
    [[ -n "${CALLGRAPH_BIN:-}" ]] && { echo "$CALLGRAPH_BIN"; return; }
    local os arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"; arch="$(uname -m)"
    case "$os" in mingw*|msys*|cygwin*) os=windows ;; esac   # Git Bash: MINGW64_NT-… → windows
    case "$arch" in amd64|AMD64) arch=x86_64 ;; esac
    for c in "$LIB_DIR/bin/$os-$arch/callgraph" "$LIB_DIR/bin/$os-$arch/callgraph.exe" "$LIB_DIR/bin/callgraph" "$LIB_DIR/bin/callgraph.exe"; do
        [[ -x "$c" ]] && { echo "$c"; return; }
    done
    return 0   # nincs bináris: üres kimenet (set -e mellett se lépjen ki csendben a hívó)
}

# A bináris egy alparancsának futtatása (exec): a skill-mappát és a megjelenített utat a bináris env-ből kapja.
run_callgraph() {
    local bin
    bin="$(callgraph_bin)"
    if [[ -z "$bin" ]]; then
        msg "callgraph binary not found (CALLGRAPH_BIN, or $LIB_DIR/bin/<os>-<arch>/callgraph)." \
            "Nem található a callgraph bináris (CALLGRAPH_BIN, vagy $LIB_DIR/bin/<os>-<arch>/callgraph)." >&2
        exit 1
    fi
    CALLGRAPH_SKILL_DIR="$LIB_DIR" PHP_AST_HOME="$PHP_AST_HOME" exec "$bin" "$@"
}

# Kompatibilitás a régi, lib.sh-t source-oló scriptekkel: alias → gyökér / projektenkénti beállítás (üres, ha nincs).
resolve_project_root() {
    local bin; bin="$(callgraph_bin)"
    CALLGRAPH_SKILL_DIR="$LIB_DIR" CALLGRAPH_REGISTRY="$1" "$bin" project resolve "$2" 2>/dev/null || true
}

resolve_project_setting() {
    local bin; bin="$(callgraph_bin)"
    CALLGRAPH_SKILL_DIR="$LIB_DIR" CALLGRAPH_REGISTRY="$1" "$bin" project get "$2" "$3" 2>/dev/null || true
}

# abszolút, fizikai (symlink-feloldott) útvonal — létező fájlra / mappára (a régi scriptek használják)
abs_path() {
    local p="$1"
    if [[ -d "$p" ]]; then (cd "$p" && pwd -P); else printf '%s/%s\n' "$(cd "$(dirname "$p")" && pwd -P)" "$(basename "$p")"; fi
}

# Hibaüzenet + error-id (napló: errors.jsonl), majd exit 1 — a `callgraph error` végzi (jq nélkül).
fail() {
    local context="$1" message="${2//\~\/.claude\/skills\/callgraph/$PHP_AST_HOME}" bin
    bin="$(callgraph_bin)"
    if [[ -n "$bin" ]]; then
        CALLGRAPH_SKILL_DIR="$LIB_DIR" PHP_AST_HOME="$PHP_AST_HOME" "$bin" error "$(basename "$0")" "$context" "$message"
    else
        echo "$message" >&2
    fi
    exit 1
}
