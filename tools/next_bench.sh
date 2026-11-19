#!/usr/bin/env bash
# next_bench.sh - A/B task benchmark for the `next` suggestions: the same task set run twice with a headless
# Claude Code agent, once with `next` OFF and once with `next` + learning ON (CALLGRAPH_NEXT=1 CALLGRAPH_NEXT_LEARN=1),
# then a comparison of task success and tokens. Needs an API key and costs money: run it only on the owner's word.
# Results stay outside the repo (private). Thin wrapper: the agent is `claude -p`, success is a per-task shell check.
#
#   tools/next_bench.sh <tasks-dir> <project-dir> [out-dir]
#
# <tasks-dir>: one subfolder per task with `prompt.txt` (what the agent is asked) and `check.sh` (exit 0 = success;
#   gets the agent's final answer on stdin). <project-dir>: a READ-ONLY COPY of the project (callgraph writes
#   .callgraph/ into it; never point this at an original). Runs are interleaved per task (off, on) so drift hits
#   both arms alike. Each run starts from an empty suggestion log unless CALLGRAPH_NEXT_SEED_LOG is a directory with
#   log*.jsonl files to copy into <project>/.callgraph/results/ first (so the learner has something to learn from).
# Env: CLAUDE_BIN (default `claude`), RUNS (per task and arm, default 3). The agent uses the installed callgraph
#   skill; the two arms differ only in the two environment variables.
set -u

TASKS="${1:-}"; PROJECT="${2:-}"; OUT="${3:-./next-bench-out}"
if [[ -z "$TASKS" || -z "$PROJECT" || ! -d "$TASKS" || ! -d "$PROJECT" ]]; then
    echo "Usage: $0 <tasks-dir> <project-dir (read-only copy)> [out-dir]" >&2
    exit 2
fi
if [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
    echo "ANTHROPIC_API_KEY is not set (this script calls the API)." >&2
    exit 2
fi
CLAUDE="${CLAUDE_BIN:-claude}"
RUNS="${RUNS:-3}"
mkdir -p "$OUT"
LOG_DIR="$PROJECT/.callgraph/results"

run_arm() { # arm task run
    local arm="$1" task="$2" run="$3"
    local dir="$TASKS/$task"
    local base="$OUT/$task.$arm.$run"
    rm -rf "$LOG_DIR"
    mkdir -p "$LOG_DIR"
    if [[ "$arm" == "on" && -n "${CALLGRAPH_NEXT_SEED_LOG:-}" ]]; then
        cp "$CALLGRAPH_NEXT_SEED_LOG"/log*.jsonl "$LOG_DIR/" 2>/dev/null
    fi
    if [[ "$arm" == "on" ]]; then
        export CALLGRAPH_NEXT=1 CALLGRAPH_NEXT_LEARN=1
    else
        unset CALLGRAPH_NEXT CALLGRAPH_NEXT_LEARN
    fi
    (cd "$PROJECT" && "$CLAUDE" -p "$(cat "$dir/prompt.txt")" --output-format json) > "$base.json" 2> "$base.err"
    # success: the task's own check on the final answer
    python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1])).get("result",""))' "$base.json" 2>/dev/null \
        | bash "$dir/check.sh" > /dev/null 2>&1
    echo "${PIPESTATUS[1]}" > "$base.rc"
}

for task_dir in "$TASKS"/*/; do
    task="$(basename "$task_dir")"
    [[ -f "$task_dir/prompt.txt" && -f "$task_dir/check.sh" ]] || continue
    for run in $(seq 1 "$RUNS"); do
        run_arm off "$task" "$run"
        run_arm on "$task" "$run"
    done
done

# comparison per arm: runs, successes, tokens per run (from the JSON `usage`)
python3 -I - "$OUT" <<'PY'
import glob, json, os, sys
out = sys.argv[1]
stat = {"off": [0, 0, 0], "on": [0, 0, 0]}  # runs, ok, tokens
for rc in glob.glob(os.path.join(out, "*.rc")):
    base = rc[:-3]
    arm = base.split(".")[-2]
    try:
        usage = json.load(open(base + ".json")).get("usage", {})
    except Exception:
        usage = {}
    keys = ("input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens")
    s = stat[arm]
    s[0] += 1
    s[1] += int(open(rc).read().strip() == "0")
    s[2] += sum(usage.get(k, 0) for k in keys)
for arm in ("off", "on"):
    runs, ok, tokens = stat[arm]
    print(f"{arm:>3}: {runs} runs, success {ok}/{runs}, tokens/run {tokens // max(runs, 1)}")
PY
