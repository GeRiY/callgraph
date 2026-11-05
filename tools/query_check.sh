#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CASES="${CASES:-$ROOT/golden/query/cases.tsv}"
GOLDEN="${GOLDEN:-$ROOT/golden/query/out}"
MAP="${CALLGRAPH_MAP:-$ROOT/golden/query/rpg_map.tsv}"
TEMPLATE="${1:-}"
ONLY=""
shift $(( $# > 0 ? 1 : 0 ))
if [[ "${1:-}" == "--only" ]]; then ONLY="${2:-}"; fi

if [[ -z "$TEMPLATE" ]]; then
    echo "Használat: $0 '<callgraph-parancs-sablon>' [--only eset-id]" >&2
    exit 2
fi

# A golden kimenet a régi (magyar) callgraph skillé: a szöveges kimenet nyelvét magyarra rögzítjük, hogy a paritás
# magyar–magyar összevetés maradjon (a bináris alapból angolul ír).
export CALLGRAPH_LANG=hu

lookup_map() {
    local wanted="$1"
    [[ -f "$MAP" ]] || return 1
    awk -F '\t' -v id="$wanted" '$1 == id { print substr($0, index($0, FS)+1); exit }' "$MAP"
}

same=0; different=0; errors=0; noport=0; total=0
tmp="$(mktemp -d "${TMPDIR:-/tmp}/query-check.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

while IFS=$'\t' read -r case_id _; do
    [[ -z "$case_id" || "$case_id" == \#* ]] && continue
    [[ -n "$ONLY" && "$case_id" != "$ONLY" ]] && continue
    total=$((total + 1))
    args="$(lookup_map "$case_id" || true)"
    if [[ -z "$args" ]]; then
        printf '%-28s NINCS PORT\n' "$case_id"
        noport=$((noport + 1)); continue
    fi
    base="$tmp/$case_id"
    # {args} is replaced when present; otherwise mapped arguments are appended.
    if [[ "$TEMPLATE" == *'{args}'* ]]; then
        command_line="${TEMPLATE//\{args\}/$args}"
    else
        command_line="$TEMPLATE $args"
    fi
    bash -c "$command_line" >"$base.txt" 2>"$base.err"
    rc=$?
    sed -E -i '' -e 's/\[error-id: [^]]+\]/[error-id: X]/g' -e 's/(feedback\.sh add )[0-9a-f]+/\1X/g' "$base.err" 2>/dev/null || \
        sed -E -i -e 's/\[error-id: [^]]+\]/[error-id: X]/g' -e 's/(feedback\.sh add )[0-9a-f]+/\1X/g' "$base.err"
    printf '%s\n' "$rc" >"$base.rc"
    if diff -q "$GOLDEN/$case_id.txt" "$base.txt" >/dev/null && \
       diff -q "$GOLDEN/$case_id.err" "$base.err" >/dev/null && \
       diff -q "$GOLDEN/$case_id.rc" "$base.rc" >/dev/null; then
        printf '%-28s AZONOS\n' "$case_id"; same=$((same + 1))
    else
        printf '%-28s ELTÉR\n' "$case_id"; different=$((different + 1))
        if [[ "$rc" -ne 0 ]]; then errors=$((errors + 1)); fi
        diff -u "$GOLDEN/$case_id.txt" "$base.txt" || true
        diff -u "$GOLDEN/$case_id.err" "$base.err" || true
        diff -u "$GOLDEN/$case_id.rc" "$base.rc" || true
    fi
done <"$CASES"

printf '\nÖsszesítő: %d eset, AZONOS=%d, ELTÉR=%d, HIBA=%d, NINCS PORT=%d\n' \
    "$total" "$same" "$different" "$errors" "$noport"
(( different == 0 ))
