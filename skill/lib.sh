#!/usr/bin/env bash
# lib.sh — közös hibakezelés a callgraph skill bash scriptjeihez (callgraph.sh,
# projects.sh). Nem önállóan futtatandó — source-old be: `source "$SCRIPT_DIR/lib.sh"`
#
# Minden hiba-kilépési pontnál `fail <context> <message>`-t hívva: a hiba
# egyedi ID-vel naplózódik az errors.jsonl-be, a hibaüzenet mellé kiírja az
# ID-t és a visszajelzés-parancsot, majd exit 1. Lásd: feedback.sh

# LIB_DIR: a script TÉNYLEGES, fizikai fájlrendszeri helye — ez mindig a
# valódi self-location, sosem felülbírálható, mert innen töltjük be a saját
# fájljainkat (vendor/, feedback.sh, errors.jsonl, cache/). Alapból ("a script
# a saját mappájában állítson be cwd-t") ez magától kiderül, függetlenül
# attól, honnan hívták a scriptet.
LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# PHP_AST_HOME: a hibaüzenetekben/help-ben MEGJELENÍTETT elérési út — alapból
# megegyezik LIB_DIR-rel, de env-változóval felülbírálható, ha a skill máshova
# van telepítve/symlinkelve, és a kiírt "bash .../callgraph.sh ..." javaslatok
# egy másik (pl. kanonikus) útvonalat mutassanak. A tényleges fájlműveleteket
# ez SOHA nem befolyásolja — azokhoz mindig LIB_DIR a forrás.
PHP_AST_HOME="${PHP_AST_HOME:-$LIB_DIR}"

# A hívó script fejléc-kommentjét (a shebang utáni "#" sorokat, a "# " prefix
# nélkül) nyomtatja ki — ez maga a script tetején lévő Használat-leírás, így
# nincs külön karbantartandó help-szöveg. Hívd: print_help "${BASH_SOURCE[0]}"
print_help() {
    local script="$1"
    awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "$script" \
        | sed "s|~/.claude/skills/callgraph|$PHP_AST_HOME|g"
}

# projects.json bejegyzés: vagy sima string (abszolút útvonal — a régi,
# egyszerű forma), vagy objektum {"path": "...", <egyéb projektenkénti
# beállítás, pl. "phpstan_level", "phpstan_config">}. A két segédfüggvény
# mindkét formát kezeli, hogy a régi (string) bejegyzések ne törjenek el.

# Egy projekt-alias gyökér-útvonala a registry-ből. Üres, ha nincs ilyen alias.
# abszolút, fizikai (symlink-feloldott) útvonal — létező fájlra / mappára
abs_path() {
    local p="$1"
    if [[ -d "$p" ]]; then (cd "$p" && pwd -P); else printf '%s/%s\n' "$(cd "$(dirname "$p")" && pwd -P)" "$(basename "$p")"; fi
}

resolve_project_root() {
    local registry="$1" alias="$2"
    jq -r --arg a "$alias" '.[$a] | if type == "object" then (.path // "") else (. // "") end' "$registry"
}

# Egy projekt-alias EXTRA beállítás-kulcsa (pl. phpstan_level) a registry-ből.
# Üres, ha a bejegyzés sima string (nincs extra beállítása), vagy a kulcs nincs beállítva.
resolve_project_setting() {
    local registry="$1" alias="$2" key="$3"
    jq -r --arg a "$alias" --arg k "$key" '.[$a] | if type == "object" then (.[$k] // "") else "" end' "$registry"
}

# Hiba naplózása ID-val (nem lép ki — használd `fail`-t, ha ki is kell lépni).
log_error() {
    local context="$1"
    local message="$2"
    local id
    id="$(openssl rand -hex 4 2>/dev/null || printf '%08x' "$RANDOM$RANDOM")"
    local ts
    ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    jq -nc --arg id "$id" --arg ts "$ts" --arg source "$(basename "$0")" \
        --arg context "$context" --arg message "$message" \
        '{id:$id, ts:$ts, source:$source, context:$context, message:$message}' \
        >> "$LIB_DIR/errors.jsonl"
    echo "$id"
}

# Hibaüzenet kiírása (+ error-id és visszajelzés-útmutató), majd exit 1.
fail() {
    local context="$1"
    local message="${2//\~\/.claude\/skills\/callgraph/$PHP_AST_HOME}"
    local id
    id="$(log_error "$context" "$message")"
    echo "$message" >&2
    echo "[error-id: $id]  visszajelzés: bash $PHP_AST_HOME/feedback.sh add $id \"<mit vártál, mi történt>\"" >&2
    exit 1
}
