#!/usr/bin/env bash
# callgraph.sh — thin wrapper over the callgraph binary (`callgraph wrap`): compact PHP / JS / TS / Vue file context
# (outline, call graph, method bodies, "where is it called") without reading the file.
# `callgraph.sh lang [en|hu]` shows / sets the output language (forwarded to `callgraph lang`).
# Help: skill/help/callgraph.en.txt and skill/help/callgraph.hu.txt (`callgraph.sh --help`).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

# `lang` is the language command, unless it names an existing path (e.g. a Laravel `lang/` dir):
#   callgraph.sh lang          → language command if there is no ./lang
#   callgraph.sh lang <code>   → language command if <code> looks like en / hu / hu_HU… and is not a path
# To analyse a `lang` dir anyway: callgraph.sh ./lang (or lang/).
is_lang_command() {
    [[ "${1:-}" == lang ]] || return 1
    case $# in
        1) [[ ! -e lang ]] ;;
        2) [[ "$2" =~ ^[A-Za-z]{2}([_.-].*)?$ && ! -e "$2" ]] ;;
        *) return 1 ;;
    esac
}
if is_lang_command "$@"; then
    shift
    run_callgraph lang "$@"
fi
for a in "$@"; do
    case "$a" in --help|-h|help) print_help "${BASH_SOURCE[0]}"; exit 0 ;; esac
done
run_callgraph wrap "$@"
