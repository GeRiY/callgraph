#!/usr/bin/env bash
# projects.sh — projekt-aliasok kezelése a callgraph skillhez
#
# A registry (projects.json) egy alias => abszolút projekt-gyökér útvonal map.
# Ez teszi lehetővé, hogy a callgraph.sh-t rövid, projekten belüli relatív
# úttal hívd meg a teljes abszolút útvonal helyett:
#   bash ~/.claude/skills/callgraph/callgraph.sh --project myapp app/Models/Order.php
# ahelyett hogy:
#   bash ~/.claude/skills/callgraph/callgraph.sh /path/to/myapp/app/Models/Order.php
#
# Használat:
#   bash ~/.claude/skills/callgraph/projects.sh add <alias> <abszolút_útvonal>
#   bash ~/.claude/skills/callgraph/projects.sh list
#   bash ~/.claude/skills/callgraph/projects.sh remove <alias>
#   bash ~/.claude/skills/callgraph/projects.sh set-config <alias> [--phpstan-level N] [--phpstan-config <út>]
#     Projektenkénti alapértelmezés analyze.sh-hoz (PHPStan szint/config) —
#     utána "bash analyze.sh --project <alias> ..." ezt használja, ha a
#     hívó nem ad meg explicit --level/--config-ot.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REGISTRY="$SCRIPT_DIR/projects.json"
source "$SCRIPT_DIR/lib.sh"

if [[ ! -f "$REGISTRY" ]]; then
    echo '{}' > "$REGISTRY"
fi

CMD="${1:-}"

case "$CMD" in
    --help|-h|help)
        print_help "${BASH_SOURCE[0]}"
        exit 0
        ;;

    add)
        ALIAS="${2:-}"
        PATH_ARG="${3:-}"
        if [[ -z "$ALIAS" || -z "$PATH_ARG" ]]; then
            fail "usage" "Használat: bash ~/.claude/skills/callgraph/projects.sh add <alias> <abszolút_útvonal>"
        fi
        if [[ "$PATH_ARG" != /* ]]; then
            fail "usage" "Az útvonalnak abszolútnak kell lennie (/-vel kezdődjön): $PATH_ARG"
        fi
        if [[ ! -d "$PATH_ARG" ]]; then
            fail "not_found" "Nem létező mappa: $PATH_ARG"
        fi
        TMP="$(mktemp)"
        jq --arg a "$ALIAS" --arg p "$PATH_ARG" '.[$a] = $p' "$REGISTRY" > "$TMP" && mv "$TMP" "$REGISTRY"
        echo "hozzáadva: $ALIAS -> $PATH_ARG"
        ;;

    remove)
        ALIAS="${2:-}"
        if [[ -z "$ALIAS" ]]; then
            fail "usage" "Használat: bash ~/.claude/skills/callgraph/projects.sh remove <alias>"
        fi
        TMP="$(mktemp)"
        jq --arg a "$ALIAS" 'del(.[$a])' "$REGISTRY" > "$TMP" && mv "$TMP" "$REGISTRY"
        echo "eltávolítva: $ALIAS"
        ;;

    set-config)
        ALIAS="${2:-}"
        if [[ -z "$ALIAS" ]]; then
            fail "usage" "Használat: bash ~/.claude/skills/callgraph/projects.sh set-config <alias> [--phpstan-level N] [--phpstan-config <út>]"
        fi
        if ! jq -e --arg a "$ALIAS" 'has($a)' "$REGISTRY" > /dev/null; then
            fail "not_found" "Nincs '$ALIAS' regisztrált projekt — előbb: bash projects.sh add $ALIAS <abszolút_útvonal>"
        fi
        shift 2
        PS_LEVEL=""
        PS_CONFIG=""
        while [[ $# -gt 0 ]]; do
            case "$1" in
                --phpstan-level)  PS_LEVEL="$2"; shift 2 ;;
                --phpstan-config) PS_CONFIG="$2"; shift 2 ;;
                *) fail "usage" "Ismeretlen kapcsoló: $1 (várt: --phpstan-level | --phpstan-config)" ;;
            esac
        done
        if [[ -z "$PS_LEVEL" && -z "$PS_CONFIG" ]]; then
            fail "usage" "Adj meg legalább egyet: --phpstan-level N | --phpstan-config <út>"
        fi
        TMP="$(mktemp)"
        jq --arg a "$ALIAS" --arg lvl "$PS_LEVEL" --arg cfg "$PS_CONFIG" '
            (.[$a] | if type == "object" then . else {path: .} end) as $entry |
            .[$a] = ($entry
                | if $lvl != "" then .phpstan_level = $lvl else . end
                | if $cfg != "" then .phpstan_config = $cfg else . end)
        ' "$REGISTRY" > "$TMP" && mv "$TMP" "$REGISTRY"
        echo "beállítva: $ALIAS"
        jq --arg a "$ALIAS" '.[$a]' "$REGISTRY"
        ;;

    list|"")
        jq -r 'to_entries[] | "\(.key)\t" + (if (.value|type)=="object" then .value.path + "  [phpstan_level=" + (.value.phpstan_level // "-") + " phpstan_config=" + (.value.phpstan_config // "-") + "]" else .value end)' "$REGISTRY"
        ;;

    *)
        fail "usage" "Ismeretlen parancs: $CMD (add|list|remove|set-config)"
        ;;
esac
