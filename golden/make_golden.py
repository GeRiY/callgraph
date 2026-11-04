#!/usr/bin/env python3
"""Create JSONL golden output for every PHP file listed in corpus.txt."""
import json
import argparse
import os
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parent
CORPUS = ROOT / "corpus.txt"
REF = ROOT / "ref.jsonl"
SCRIPT = Path.home() / ".claude/skills/callgraph/ast_graph.php"


def one_file(path):
    try:
        proc = subprocess.run(
            ["php", "-d", "display_errors=0", str(SCRIPT), path],
            capture_output=True, text=True, encoding="utf-8",
        )
        if proc.returncode != 0:
            raise RuntimeError("exit %d: %s" % (proc.returncode, proc.stderr.strip()))
        data = json.loads(proc.stdout)
        if not isinstance(data, dict):
            raise ValueError("JSON output is not an object")
        return {"path": path, "graph": data}
    except (OSError, ValueError, json.JSONDecodeError, RuntimeError) as exc:
        return {"path": path, "error": str(exc)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", default=str(CORPUS))
    ap.add_argument("--out", default=str(REF))
    args = ap.parse_args()
    corpus = Path(args.corpus)
    paths = [line.strip() for line in corpus.read_text(encoding="utf-8").splitlines() if line.strip()]
    batched = {}
    # a két referencia-korpusz: tools/local.env (CALLGRAPH_CORPUS_A / _B) — környezeti változóként add át
    batch_roots = [r for r in (os.environ.get("CALLGRAPH_CORPUS_A"), os.environ.get("CALLGRAPH_CORPUS_B")) if r]
    if str(corpus) != str(CORPUS) and paths and all(any(path.startswith(root + "/") for root in batch_roots) for path in paths):
        for root in batch_roots:
            if not any(path.startswith(root + "/") for path in paths):
                continue
            proc = subprocess.run(
                ["php", "-d", "display_errors=0", str(SCRIPT), root],
                capture_output=True, text=True, encoding="utf-8",
            )
            if proc.returncode == 0:
                for line in proc.stdout.splitlines():
                    try:
                        graph = json.loads(line)
                        if isinstance(graph, dict) and isinstance(graph.get("file"), str):
                            batched[graph["file"]] = {"path": graph["file"], "graph": graph}
                    except json.JSONDecodeError:
                        pass
    with Path(args.out).open("w", encoding="utf-8") as output:
        for path in paths:
            entry = batched[path] if path in batched else one_file(path)
            output.write(json.dumps(entry, ensure_ascii=False, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()
