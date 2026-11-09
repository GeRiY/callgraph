#!/usr/bin/env bash
# callgraph.sh — thin wrapper over the callgraph binary (`callgraph wrap`): compact PHP / JS / TS / Vue file context
# (outline, call graph, method bodies, "where is it called") without reading the file.
# `callgraph.sh set-lang [en|hu]` shows / sets the output language (forwarded to `callgraph set-lang`).
# `callgraph.sh update [--check] [--dev|--force]` updates the skill from GitHub Releases (`--dev`: newest develop build) (forwarded to `callgraph self-update`).
# `callgraph.sh note add|list|rm …` (pilot) manages notes on files / methods (forwarded to `callgraph note`).
# `callgraph.sh refresh [--project alias] [--background] [<path>…]` brings the project graph up to date
# (forwarded to `callgraph refresh`); `callgraph.sh hook install|uninstall|status` manages the Claude Code hook.
# `callgraph.sh locate <term>… [--all] [--project alias]` lists where to start with a task (ranked files + reasons;
# forwarded to `callgraph locate`).
# `callgraph.sh profile [--project alias] [--full]` prints the project profile (forwarded to `callgraph profile`).
# `callgraph.sh stats [--days N]` shows the local usage stats (forwarded to `callgraph stats`; nothing is sent anywhere).
# `callgraph.sh --version` (or `-V`, `version`) prints the binary's version and platform, and the binary's path.
# `--no-gitignore` with any command: the .gitignore files are not applied (.callgraphignore still is).
# Help: skill/help/callgraph.en.txt and skill/help/callgraph.hu.txt (`callgraph.sh --help`).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

if [[ "${1:-}" == set-lang ]]; then
    shift
    run_callgraph set-lang "$@"
fi
if [[ "${1:-}" == update ]]; then
    shift
    run_callgraph self-update "$@"
fi
# jegyzetek (pilot): `callgraph.sh note add|list|rm …` → `callgraph note …` (a --project aliast a bináris oldja fel)
if [[ "${1:-}" == note ]]; then
    run_callgraph "$@"
fi
# előre-frissítés: `callgraph.sh refresh [--project alias] [--background] [<út>…]` → `callgraph refresh …`;
# a Claude Code-hook kezelése: `callgraph.sh hook install|uninstall|status` → `callgraph hook …`
if [[ "${1:-}" == refresh || "${1:-}" == hook ]]; then
    run_callgraph "$@"
fi
# kiindulópont-kereső: `callgraph.sh [--project alias] locate <kifejezés>… [--all]` → `callgraph locate …`
if [[ "${1:-}" == --project && "${3:-}" == locate ]]; then
    run_callgraph locate --project "$2" "${@:4}"
fi
if [[ "${1:-}" == locate ]]; then
    run_callgraph "$@"
fi
# projekt-profil: `callgraph.sh [--project alias] profile [--full]` → `callgraph profile …` (architektúra, rétegek, minták)
if [[ "${1:-}" == --project && "${3:-}" == profile ]]; then
    run_callgraph profile --project "$2" "${@:4}"
fi
if [[ "${1:-}" == profile ]]; then
    run_callgraph "$@"
fi
# helyi használati statisztika: `callgraph.sh stats [--days N]` → `callgraph stats …` (hálózat nincs)
if [[ "${1:-}" == stats ]]; then
    run_callgraph "$@"
fi
# verzió: `callgraph.sh --version|-V|version` → `callgraph --version` + a bináris helye (nem a show-súgó)
if [[ $# -eq 1 && ( "$1" == --version || "$1" == -V || "$1" == version ) ]]; then
    bin="$(callgraph_bin)"
    [[ -n "$bin" ]] || run_callgraph --version   # nincs bináris: a run_callgraph hibaüzenete, kilépési kód 1
    CALLGRAPH_SKILL_DIR="$LIB_DIR" "$bin" --version || exit $?
    msg "binary: $bin" "bináris: $bin"
    exit 0
fi
for a in "$@"; do
    case "$a" in --help|-h|help) print_help "${BASH_SOURCE[0]}"; exit 0 ;; esac
done
run_callgraph wrap "$@"
