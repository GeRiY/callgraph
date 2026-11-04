#!/usr/bin/env bash
# callgraph.sh — vékony burok az callgraph (Rust) fölött; tömör, token-hatékony PHP / JS / TS / Vue fájl-kontextus agentek számára,
# (a PHP-t, a .vue/.js/.ts fájlokat és a CSS/SCSS/JSON/MD-t az callgraph elemzi, egy sémában;
#  Vue-n: props, emits (emit:<név>), template (komponensek, eseménykezelők), setup, hookok;
#  kereshető kulcsok: route::<név>, i18n::<kulcs>, vue::emit:<név>, <fájl>::<export>)
# a fájl beolvasása NÉLKÜL (metódusok + hívás-gráf, kategorizálva).
#
# Használat:
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl>                    outline (alapértelmezett) — lásd lent
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl> --format outline    ua., explicit
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl> --format api        publikus szerződés + docblock, hívás-gráf NÉLKÜL
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl> --format stats      kategória-statisztika
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl> --format mermaid    Mermaid flowchart (vizualizációhoz)
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl> --format dot        Graphviz DOT (vizualizációhoz)
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl> --format json       nyers kategorizált JSON (ast_graph.php kimenete)
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl>[::metódus] --format flow  if/match/switch ágak sortartománnyal (feltétel → mit hív)
#   bash ~/.claude/skills/callgraph/callgraph.sh <mappa> --format summary   fájlonként 1 sor (osztály, metódusszám, docblock)
#   bash ~/.claude/skills/callgraph/callgraph.sh <mappa> --format coverage  dokumentálatlan publikus metódusok
#   bash ~/.claude/skills/callgraph/callgraph.sh <mappa> --format hotspots  Top 20 metódus fokszám szerint (in/out) — hol nézz körül elsőként
#   bash ~/.claude/skills/callgraph/callgraph.sh <mappa|fájl1 fájl2 ...> --format outline
#                                                 több fájl/mappa — outline-ok egymás után
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl>::<metódusnév>       CSAK egy metódus — a legtömörebb forma
#                                                 nagy fájloknál, ha egy konkrét metódus érdekel
#                                                 (bármelyik --format-tal kombinálható)
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl|mappa> --grep <minta>       csak a mintára illeszkedő célú hívások
#                                                 (csak az illeszkedő hívást tartalmazó metódusok/fájlok — "hol hívják")
#   bash ~/.claude/skills/callgraph/callgraph.sh <mappa> --grep <minta> --depth N [--guess]
#                                                 hívók hívói N szintig (hatáselemzés); --guess: a feloldatlan,
#                                                 csak név szerinti egyezéseket is ("?")
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl>::<metódus> [...] --body   a metódus(ok) forrása sorszámozva, docblockkal
#   bash ~/.claude/skills/callgraph/callgraph.sh <mappa> --grep <minta> --body    a talált hívó metódusok forrása
#   bash ~/.claude/skills/callgraph/callgraph.sh <fájl> --lines <n>-<m>           tetszőleges sáv sorszámozva (pl. view-fájl)
#   bash ~/.claude/skills/callgraph/callgraph.sh <mappa|fájl> --strings <minta>   string-literálok (hibaüzenet, kulcs) helye
#   bash ~/.claude/skills/callgraph/callgraph.sh <mappa|fájl> --const <minta>     osztálykonstans-hivatkozások és -definíciók
#
# Kimenet-korlát: 400 sor fölött a mappás outline summary-ra, az egyfájlos metódus-indexre vált;
#
# Motor: minden célra az `callgraph show`. A projekt-alias feloldása (projects.json) és a hibák error-id-ja (lib.sh fail) a buroké;
# az callgraph csak abszolút gyökeret és a hozzá relatív utakat kapja.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REGISTRY="${CALLGRAPH_REGISTRY:-$SCRIPT_DIR/projects.json}"
source "$SCRIPT_DIR/lib.sh"

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" || "${1:-}" == "help" ]]; then
    print_help "${BASH_SOURCE[0]}"
    exit 0
fi

# Az callgraph bináris: env-ből, vagy a skill bin/ mappájából platform szerint (bin/<os>-<arch>/callgraph), vagy bin/callgraph.
callgraph_bin() {
    [[ -n "${CALLGRAPH_BIN:-}" ]] && { echo "$CALLGRAPH_BIN"; return; }
    local os arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"; arch="$(uname -m)"
    case "$os" in mingw*|msys*|cygwin*) os=windows ;; esac   # Git Bash: MINGW64_NT-… → windows
    case "$arch" in amd64|AMD64) arch=x86_64 ;; esac
    for c in "$SCRIPT_DIR/bin/$os-$arch/callgraph" "$SCRIPT_DIR/bin/$os-$arch/callgraph.exe" "$SCRIPT_DIR/bin/callgraph" "$SCRIPT_DIR/bin/callgraph.exe"; do
        [[ -x "$c" ]] && { echo "$c"; return; }
    done
}

ORIG=("$@")
# KULCS=érték alakú elöl álló argumentum (pl. CALLGRAPH_OUT_LIMIT=50) környezeti változóként él tovább.
while [[ $# -gt 0 && "$1" =~ ^[A-Z_][A-Z0-9_]*= ]]; do export "$1"; ORIG=("${ORIG[@]:1}"); shift; done
PROJECT=""
DEPTH=1
FORWARD=()   # az callgraph-nek továbbadott kapcsolók
TARGETS=()   # a fájl/mappa argumentumok (a :: szelektorral együtt)
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help|-h|help) print_help "${BASH_SOURCE[0]}"; exit 0 ;;
        --project) PROJECT="${2:-}"; shift 2 ;;
        --depth) DEPTH="${2:-}"; FORWARD+=("$1" "${2:-}"); shift 2 ;;
        --format|--grep|--category|--lines|--strings|--const) FORWARD+=("$1" "${2:-}"); shift 2 ;;
        --body|--full|--guess) FORWARD+=("$1"); shift ;;
        *) TARGETS+=("$1"); shift ;;
    esac
done
[[ "$DEPTH" =~ ^[1-9][0-9]*$ ]] || fail "usage" "A --depth pozitív egész (kaptam: $DEPTH)."
if [[ ${#TARGETS[@]} -eq 0 ]]; then
    fail "usage" "Használat: bash ~/.claude/skills/callgraph/callgraph.sh <fájl|mappa>[::metódusnév] [...] [--project alias] [--format outline|api|stats|mermaid|dot|json]"
fi

ROOT="$PWD"
if [[ -n "$PROJECT" ]]; then
    [[ -f "$REGISTRY" ]] || fail "usage" "Nincs projects.json — regisztrálj előbb: bash ~/.claude/skills/callgraph/projects.sh add $PROJECT <abszolút_útvonal>"
    ROOT="$(resolve_project_root "$REGISTRY" "$PROJECT")"
    [[ -n "$ROOT" ]] || fail "usage" "Ismeretlen projekt-alias: $PROJECT (lásd: bash ~/.claude/skills/callgraph/projects.sh list)"
fi


RPG="$(callgraph_bin)"
[[ -n "$RPG" ]] || fail "usage" "Nem található az callgraph bináris (CALLGRAPH_BIN, vagy $SCRIPT_DIR/bin/<os>-<arch>/callgraph)."

ERR="$(mktemp "${TMPDIR:-/tmp}/callgraph-callgraph.XXXXXX")"
trap 'rm -f "$ERR"' EXIT
"$RPG" show --project "$ROOT" --label "${PROJECT:-$ROOT}" "${FORWARD[@]+"${FORWARD[@]}"}" "${TARGETS[@]}" 2>"$ERR"
rc=$?
if (( rc != 0 )); then
    msg="$(cat "$ERR")"
    case "$msg" in
        "Nem található"*|"Nincs ilyen metódus"*) fail "not_found" "$msg" ;;
        *) fail "usage" "$msg" ;;
    esac
fi
exit 0
