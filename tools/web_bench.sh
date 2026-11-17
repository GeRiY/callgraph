#!/usr/bin/env bash
# Webes futásidő- és kimenet-méret mérőalap (a webes javítási terv 4. fejezete): minden megadott projekt-mappán
# lefuttatja a fix parancslistát, parancsonként a legjobb idő (alapból 3 futásból) és a kimenet bájt / sor száma.
# A kimenet TSV a szabványos kimenetre: projekt, parancs, ms, bájt, sor, állapot, jelzés.
# A projekt-mappába csak a callgraph saját cache-e (.callgraph/) kerül.
#
#   tools/web_bench.sh [--baseline <elozo.tsv>] [--runs N] <callgraph-bináris> <projekt-mappa>...
#
# --baseline: egy korábbi futás TSV-jéhez hasonlít; jelzés: SLOW (idő > +20 %, legalább 5 ms eltéréssel) és BYTES
#   (a kimenet bájtja nőtt). Ha van jelzés, a kilépési kód 1.
# Környezet: WEB_BENCH_GREP (a --depth 3 keresés mintája, alapból az `error` nevű hívások), WEB_BENCH_LOCATE
#   (a locate szavai, szóközzel elválasztva, alapból "route error").
# A referencia-TSV a repón kívül marad (a mérőszámok nem kerülnek a repóba).
set -u

BASELINE=""
RUNS=3
while [[ $# -gt 0 ]]; do
    case "$1" in
        --baseline) BASELINE="${2:-}"; shift 2 ;;
        --runs) RUNS="${2:-3}"; shift 2 ;;
        *) break ;;
    esac
done

BIN="${1:-}"
if [[ -z "$BIN" || $# -lt 2 ]]; then
    echo "Használat: $0 [--baseline <elozo.tsv>] [--runs N] <callgraph-bináris> <projekt-mappa>..." >&2
    exit 2
fi
shift
[[ -x "$BIN" ]] || { echo "Nem futtatható: $BIN" >&2; exit 2; }
BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"
if [[ -n "$BASELINE" && ! -f "$BASELINE" ]]; then
    echo "Nincs ilyen alapvonal-fájl: $BASELINE" >&2
    exit 2
fi

GREP_PATTERN="${WEB_BENCH_GREP:-(^|::|\.|->)error\$}"
LOCATE_TERMS="${WEB_BENCH_LOCATE:-route error}"

# Stabil kimenet: angol szöveg, nincs frissítés-ellenőrzés, nincs regisztry.
export CALLGRAPH_LANG=en CALLGRAPH_UPDATE_CHECK=0 CALLGRAPH_REGISTRY=/nonexistent/projects.json

tmp="$(mktemp -d "${TMPDIR:-/tmp}/web-bench.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
result="$tmp/result.tsv"
: > "$result"

now_ms() { echo $(( $(date +%s%N) / 1000000 )); }

# measure <projekt-név> <azonosító> <mappa> <argumentumok...>
measure() {
    local project="$1" id="$2" dir="$3"; shift 3
    local best="" status="ok" out="$tmp/out.txt" start end ms code run
    for run in $(seq 1 "$RUNS"); do
        start="$(now_ms)"
        (cd "$dir" && "$BIN" "$@" >"$out" 2>"$tmp/err.txt")
        code=$?
        end="$(now_ms)"
        ms=$((end - start))
        if [[ $code -ne 0 ]]; then status="exit$code"; fi
        if [[ -z "$best" || $ms -lt $best ]]; then best="$ms"; fi
    done
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$project" "$id" "$best" "$(wc -c < "$out" | tr -d ' ')" \
        "$(wc -l < "$out" | tr -d ' ')" "$status" >> "$result"
}

for dir in "$@"; do
    if [[ ! -d "$dir" ]]; then
        echo "Kihagyva (nem mappa): $dir" >&2
        continue
    fi
    dir="$(cd "$dir" && pwd)"
    project="$(basename "$dir")"

    measure "$project" cold "$dir" . --no-cache
    measure "$project" profile "$dir" profile
    if [[ -f "$dir/composer.json" ]]; then
        for view in ziggy forms inertia models events middleware; do
            measure "$project" "view_$view" "$dir" show . --format "$view"
        done
    else
        echo "Kihagyva ($project): nincs composer.json — a PHP framework-nézetek nem relevánsak" >&2
    fi
    for view in i18n classes summary; do
        measure "$project" "view_$view" "$dir" show . --format "$view"
    done
    measure "$project" grep_depth3 "$dir" show . --grep "$GREP_PATTERN" --depth 3
    # shellcheck disable=SC2086
    measure "$project" locate "$dir" locate $LOCATE_TERMS

    # A legnagyobb .vue fájl teljes outline-ja (a token-hatékonysági célok O1 / O1b mérőpontja).
    vue="$(find "$dir" -type f -name '*.vue' -not -path '*/node_modules/*' -not -path '*/vendor/*' \
        -not -path '*/.callgraph/*' -exec wc -l {} + 2>/dev/null | grep -v ' total$' | sort -rn | head -n 1 \
        | sed 's/^ *[0-9]* //')"
    if [[ -n "$vue" ]]; then
        measure "$project" vue_outline_full "$dir" show "$vue" --full
    else
        echo "Kihagyva ($project): nincs .vue fájl — vue_outline_full" >&2
    fi
done

# Jelzések az alapvonalhoz képest, majd a TSV a szabványos kimenetre.
flags=0
printf 'projekt\tparancs\tms\tbajt\tsor\tallapot\tjelzes\n'
if [[ -n "$BASELINE" ]]; then
    awk -F '\t' -v OFS='\t' '
        NR == FNR { if (FNR > 1) { ms[$1 SUBSEP $2] = $3; bytes[$1 SUBSEP $2] = $4 } next }
        {
            key = $1 SUBSEP $2; flag = ""
            if (key in ms) {
                if ($3 > ms[key] * 1.2 && $3 - ms[key] >= 5) flag = flag "SLOW(+" int(($3 - ms[key]) * 100 / (ms[key] ? ms[key] : 1)) "%) "
                if ($4 > bytes[key]) flag = flag "BYTES(+" ($4 - bytes[key]) ") "
            } else flag = "NEW"
            if ($6 != "ok") flag = flag $6
            print $0, flag
        }' "$BASELINE" "$result" > "$tmp/flagged.tsv"
    cat "$tmp/flagged.tsv"
    if awk -F '\t' '$7 ~ /SLOW|BYTES/ { found = 1 } END { exit !found }' "$tmp/flagged.tsv"; then flags=1; fi
else
    awk -F '\t' -v OFS='\t' '{ print $0, ($6 == "ok" ? "" : $6) }' "$result"
fi

if [[ $flags -ne 0 ]]; then
    echo "Eltérés az alapvonaltól: lásd a jelzes oszlopot (SLOW / BYTES)." >&2
    exit 1
fi
exit 0
