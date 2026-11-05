#!/usr/bin/env bash
# callgraph.sh — thin wrapper over the callgraph binary (`callgraph wrap`): compact PHP / JS / TS / Vue file context
# (outline, call graph, method bodies, "where is it called") without reading the file.
# `callgraph.sh set-lang [en|hu]` shows / sets the output language (forwarded to `callgraph set-lang`).
# Help: skill/help/callgraph.en.txt and skill/help/callgraph.hu.txt (`callgraph.sh --help`).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

if [[ "${1:-}" == set-lang ]]; then
    shift
    run_callgraph set-lang "$@"
fi
for a in "$@"; do
    case "$a" in --help|-h|help) print_help "${BASH_SOURCE[0]}"; exit 0 ;; esac
done
run_callgraph wrap "$@"
