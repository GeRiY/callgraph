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
#   bash ~/.claude/skills/callgraph/projects.sh resolve <alias>          a gyökér-útvonal (ismeretlen aliasra error-id-s hiba)
#   bash ~/.claude/skills/callgraph/projects.sh get <alias> <kulcs>      projektenkénti beállítás (pl. phpstan_level; üres, ha nincs)
#   bash ~/.claude/skills/callgraph/projects.sh set-config <alias> [--phpstan-level N] [--phpstan-config <út>]
#     Projektenkénti alapértelmezés analyze.sh-hoz (PHPStan szint/config) —
#     utána "bash analyze.sh --project <alias> ..." ezt használja, ha a
#     hívó nem ad meg explicit --level/--config-ot.

# A logika a `callgraph project` alparancsban él (jq nélkül); ez csak a súgót írja ki és továbbít.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

case "${1:-}" in --help|-h|help) print_help "${BASH_SOURCE[0]}"; exit 0 ;; esac
run_callgraph project "$@"
