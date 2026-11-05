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

# A hívó script fejléc-kommentjét (a shebang utáni "#" sorokat) nyomtatja ki — ez a súgó.
print_help() {
    local script="$1"
    awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "$script" \
        | sed "s|~/.claude/skills/callgraph|$PHP_AST_HOME|g"
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
}

# A bináris egy alparancsának futtatása (exec): a skill-mappát és a megjelenített utat a bináris env-ből kapja.
run_callgraph() {
    local bin
    bin="$(callgraph_bin)"
    if [[ -z "$bin" ]]; then
        echo "Nem található a callgraph bináris (CALLGRAPH_BIN, vagy $LIB_DIR/bin/<os>-<arch>/callgraph)." >&2
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
