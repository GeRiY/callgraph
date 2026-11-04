#!/usr/bin/env bash
# feedback.sh — visszajelzés a callgraph skillről (hibáról vagy általános
# észrevételről), agent-eknek.
#
# Hiba esetén a callgraph.sh / projects.sh
# minden hiba-üzenet mellé kiír egy "[error-id: xxxxxxxx]"
# azonosítót — a hiba kontextusa (forrás script, üzenet, paraméterek) már
# el van tárolva mellette az errors.jsonl-ben, ehhez CSAK az ID-t és egy
# leírást kell megadni:
#
#   bash ~/.claude/skills/callgraph/feedback.sh add <error_id> "<mit vártál, mi történt valójában>"
#
# Ha nincs konkrét error-id (csak egy általános ötlet/észrevétel az
# eszközről), a --note kapcsolóval önmagában is rögzíthető:
#
#   bash ~/.claude/skills/callgraph/feedback.sh add --note "<észrevétel>"
#
# Minden feedback-bejegyzés saját ID-t kap (az "elmentve" válasz kiírja) —
# ezzel jelölhető megoldottnak, ha valaki (a fejlesztő) már kezelte:
#
#   bash ~/.claude/skills/callgraph/feedback.sh resolve <feedback_id>
#
# Egyéb parancsok:
#   bash ~/.claude/skills/callgraph/feedback.sh list                minden feedback, állapottal és a kapcsolódó hibával
#   bash ~/.claude/skills/callgraph/feedback.sh list --open          csak a még nem megoldott bejegyzések
#   bash ~/.claude/skills/callgraph/feedback.sh show <error_id>      egy konkrét hiba teljes részlete + a rá érkezett feedback-ek

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

ERRORS_FILE="$SCRIPT_DIR/errors.jsonl"
FEEDBACK_FILE="$SCRIPT_DIR/feedback.jsonl"

CMD="${1:-}"

case "$CMD" in
    --help|-h|help)
        print_help "${BASH_SOURCE[0]}"
        exit 0
        ;;

    add)
        shift
        ERROR_ID=""
        DESCRIPTION=""
        if [[ "${1:-}" == "--note" ]]; then
            DESCRIPTION="${2:-}"
        else
            ERROR_ID="${1:-}"
            DESCRIPTION="${2:-}"
        fi

        if [[ -z "$DESCRIPTION" ]]; then
            fail "usage" "Használat: bash ~/.claude/skills/callgraph/feedback.sh add <error_id> \"<leírás>\"  VAGY  bash ~/.claude/skills/callgraph/feedback.sh add --note \"<leírás>\""
        fi

        if [[ -n "$ERROR_ID" ]]; then
            if [[ ! -f "$ERRORS_FILE" ]] || ! jq -e --arg id "$ERROR_ID" 'select(.id == $id)' "$ERRORS_FILE" > /dev/null 2>&1; then
                echo "figyelem: nincs '$ERROR_ID' azonosítójú naplózott hiba — a leírás így is, kapcsolat nélkül, elmentve" >&2
            fi
        fi

        FB_ID="$(openssl rand -hex 4 2>/dev/null || printf '%08x' "$RANDOM$RANDOM")"
        ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        jq -nc --arg id "$FB_ID" --arg ts "$ts" --arg eid "$ERROR_ID" --arg desc "$DESCRIPTION" \
            '{id:$id, ts:$ts, error_id: (if $eid == "" then null else $eid end), description:$desc, resolved:false, resolved_at:null}' \
            >> "$FEEDBACK_FILE"
        echo "elmentve  [feedback-id: $FB_ID]"
        ;;

    resolve)
        FB_ID="${2:-}"
        if [[ -z "$FB_ID" ]]; then
            fail "usage" "Használat: bash ~/.claude/skills/callgraph/feedback.sh resolve <feedback_id>"
        fi
        if [[ ! -f "$FEEDBACK_FILE" ]] || ! jq -e --arg id "$FB_ID" 'select(.id == $id)' "$FEEDBACK_FILE" > /dev/null 2>&1; then
            fail "not_found" "Nincs '$FB_ID' azonosítójú feedback-bejegyzés (lásd: bash ~/.claude/skills/callgraph/feedback.sh list)"
        fi
        ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        TMP="$(mktemp)"
        jq -c --arg id "$FB_ID" --arg ts "$ts" \
            'if .id == $id then .resolved = true | .resolved_at = $ts else . end' \
            "$FEEDBACK_FILE" > "$TMP" && mv "$TMP" "$FEEDBACK_FILE"
        echo "megoldva: $FB_ID"
        ;;

    list)
        OPEN_ONLY=false
        if [[ "${2:-}" == "--open" ]]; then
            OPEN_ONLY=true
        fi
        if [[ ! -f "$FEEDBACK_FILE" ]]; then
            echo "(nincs feedback)"
            exit 0
        fi
        jq -s --argjson openOnly "$OPEN_ONLY" --slurpfile errs <(cat "$ERRORS_FILE" 2>/dev/null || echo '') '
            (reduce ($errs[]?) as $e ({}; .[$e.id] = $e)) as $errIndex |
            .[] | select($openOnly == false or .resolved != true) | . as $f |
            ($f.error_id as $eid | if $eid != null then $errIndex[$eid] else null end) as $err |
            "[" + $f.ts + "]  id:" + $f.id + (if $f.resolved then "  [MEGOLDVA " + ($f.resolved_at // "") + "]" else "  [NYITOTT]" end)
                + (if $f.error_id then "  error:" + $f.error_id else "  (nincs error)" end),
            "  leírás: " + $f.description,
            (if $err then "  hiba: (" + $err.source + "/" + $err.context + ") " + $err.message else empty end),
            ""
        ' "$FEEDBACK_FILE" -r
        ;;

    show)
        ERROR_ID="${2:-}"
        if [[ -z "$ERROR_ID" ]]; then
            fail "usage" "Használat: bash ~/.claude/skills/callgraph/feedback.sh show <error_id>"
        fi
        if [[ -f "$ERRORS_FILE" ]]; then
            echo "-- hiba --"
            jq -c --arg id "$ERROR_ID" 'select(.id == $id)' "$ERRORS_FILE"
        fi
        if [[ -f "$FEEDBACK_FILE" ]]; then
            echo "-- kapcsolódó feedback --"
            jq -c --arg id "$ERROR_ID" 'select(.error_id == $id)' "$FEEDBACK_FILE"
        fi
        ;;

    *)
        fail "usage" "Használat: bash ~/.claude/skills/callgraph/feedback.sh add <error_id> \"<leírás>\" | add --note \"<leírás>\" | resolve <feedback_id> | list [--open] | show <error_id>"
        ;;
esac
