#!/usr/bin/env bash
# projects.sh — project aliases for the callgraph skill (alias => absolute root in projects.json);
# forwards to `callgraph project`.
# Help: skill/help/projects.en.txt and skill/help/projects.hu.txt (`projects.sh --help`).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

case "${1:-}" in --help|-h|help) print_help "${BASH_SOURCE[0]}"; exit 0 ;; esac
run_callgraph project "$@"
