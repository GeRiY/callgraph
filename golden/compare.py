#!/usr/bin/env python3
"""Compare candidate call-graph JSONL output with a golden corpus."""
import argparse
import json
import shlex
import subprocess
from pathlib import Path

FIELDS = ["namespace", "imports", "containers", "properties", "constants", "methods", "calls", "comments", "controlFlow", "strings", "writes", "framework", "module", "sfc"]  # module, sfc: csak JS/Vue
KEYS = {
    "containers": ("type", "name"), "methods": ("container", "name"),
    "properties": ("container", "name"), "constants": ("container", "name"),
    "calls": ("container", "caller", "line", "type", "target"),
    "controlFlow": ("container", "caller", "type", "line"),
    "strings": ("container", "caller", "line", "value"),
    "comments": ("container", "caller", "line"),
    "writes": ("container", "caller", "line", "property"),
}
# a doc (api/summary/coverage) és a body (magic) a callgraph.sh-ban használt — alapból összevetjük; --ignore-ral kihagyható
IGNORE = {"file"}


def clean(value):
    if isinstance(value, dict):
        return {k: clean(v) for k, v in value.items() if k not in IGNORE}
    if isinstance(value, list):
        return [clean(v) for v in value]
    return value


def key_for(field, item):
    return tuple(item.get(k) for k in KEYS[field])


def compare_field(field, ref, cand):
    if field not in KEYS:
        return (1 if clean(ref) == clean(cand) else 0), 1, 1, []
    r = ref if isinstance(ref, list) else []
    c = cand if isinstance(cand, list) else []
    cm = {}
    for item in c:
        cm.setdefault(key_for(field, item), []).append(item)
    matches, diffs = 0, []
    for item in r:
        k = key_for(field, item)
        choices = cm.get(k, [])
        found = next((x for x in choices if clean(x) == clean(item)), None)
        if found is not None:
            choices.remove(found); matches += 1
        else:
            diffs.append((k, item, choices[0] if choices else None))
    return matches, len(r), len(c), diffs


def load_jsonl(path):
    entries = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        entry = json.loads(line)
        if isinstance(entry, dict) and isinstance(entry.get("path"), str):
            entries[entry["path"]] = entry
    return entries


def parse_candidate_output(output):
    # JSONL contract: the first non-empty line is one JSON object.
    for line in output.splitlines():
        if line.strip():
            try:
                value = json.loads(line)
                return value if isinstance(value, dict) else {}
            except json.JSONDecodeError:
                break
    # Backward compatibility for a single pretty-printed JSON document.
    try:
        value = json.loads(output)
        return value if isinstance(value, dict) else {}
    except json.JSONDecodeError:
        return {}


def parse_candidate_jsonl(output):
    result = {}
    for line in output.splitlines():
        if not line.strip():
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(value, dict):
            continue
        path = value.get("file") or value.get("path")
        if isinstance(path, str):
            result[path] = value.get("graph", value)
    return result


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ref", required=True)
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--cmd")
    src.add_argument("--cmd-dir")
    src.add_argument("--cand")
    ap.add_argument("--dirs", help="vesszővel elválasztott mappák --cmd-dir esetén")
    ap.add_argument("--fields", default=",".join(FIELDS))
    ap.add_argument("--show", type=int, default=0)
    ap.add_argument("--ignore", default="", help="kihagyott belső kulcsok, vesszővel (pl. doc,body)")
    args = ap.parse_args()
    IGNORE.update(x for x in args.ignore.split(",") if x)
    refs = load_jsonl(args.ref)
    cands = load_jsonl(args.cand) if args.cand else {}
    if args.cmd_dir and not args.dirs:
        ap.error("--cmd-dir mellé --dirs is kell")
    if args.cmd_dir:
        for directory in (x.strip() for x in args.dirs.split(",")):
            if not directory:
                continue
            command = args.cmd_dir.replace("{dir}", shlex.quote(directory))
            proc = subprocess.run(command, shell=True, capture_output=True, text=True)
            if proc.returncode == 0:
                for path, graph in parse_candidate_jsonl(proc.stdout).items():
                    cands[path] = {"path": path, "graph": graph}
    selected = [x for x in args.fields.split(",") if x]
    totals = {f: [0, 0, 0, []] for f in selected}
    for path, entry in refs.items():
        if "error" in entry or not isinstance(entry.get("graph"), dict):
            continue
        ref = entry["graph"]
        if args.cmd:
            command = args.cmd.replace("{file}", shlex.quote(path))
            proc = subprocess.run(command, shell=True, capture_output=True, text=True)
            cand = parse_candidate_output(proc.stdout) if proc.returncode == 0 else {}
        else:
            centry = cands.get(path)
            cand = centry.get("graph", {}) if centry and "error" not in centry else {}
        for f in selected:
            m, rn, cn, diffs = compare_field(f, ref.get(f), cand.get(f))
            t = totals[f]; t[0] += m; t[1] += rn; t[2] += cn
            for d in diffs:
                if len(t[3]) < args.show: t[3].append((path, d))
    ok = True
    for f in selected:
        m, rn, cn, diffs = totals[f]; pct = 100.0 if rn == cn == m else (100.0 * m / max(rn, cn, 1))
        ok &= pct == 100.0
        print(f"{f}: {m} / {rn} / {cn} ({pct:.1f}%)")
        for path, (key, rv, cv) in diffs:
            if isinstance(rv, dict) and isinstance(cv, dict):
                # csak az eltérő kulcsok, rövidítve (a hosszú body/doc is látszik, de vágva)
                short = lambda x: (repr(x)[:120] + "…") if len(repr(x)) > 120 else repr(x)
                delta = {k: (short(rv.get(k)), short(cv.get(k))) for k in sorted(set(rv) | set(cv))
                         if k not in IGNORE and rv.get(k) != cv.get(k)}
                print(f"  {path} {key}: " + "; ".join(f"{k}: ref={a} jelölt={b}" for k, (a, b) in delta.items()))
            else:
                print(f"  {path} {key}: ref={rv!r} jelölt={cv!r}")
    print("Összesítő: " + ("100%" if ok else "eltérés"))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
