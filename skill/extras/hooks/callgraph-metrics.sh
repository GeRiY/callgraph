#!/usr/bin/env bash
# PostToolUse: kód-olvasási mérés (logika: callgraph-metrics.py). Napló: .claude/metrics/callgraph-usage.jsonl
exec python3 "$(dirname "${BASH_SOURCE[0]}")/callgraph-metrics.py"
