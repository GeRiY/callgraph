#!/usr/bin/env bash
# feedback.sh — feedback on the callgraph skill (about an error-id or a general note), for agents;
# forwards to `callgraph feedback`.
# Help: skill/help/feedback.en.txt and skill/help/feedback.hu.txt (`feedback.sh --help`).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

case "${1:-}" in --help|-h|help) print_help "${BASH_SOURCE[0]}"; exit 0 ;; esac
run_callgraph feedback "$@"
