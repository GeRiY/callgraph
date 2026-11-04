#!/usr/bin/env bash
# PHP/JS/Vue-olvasási mérés riportja (.claude/metrics/callgraph-usage.jsonl, a callgraph-metrics.sh írja).
# Használat: bash .claude/tools/callgraph-report.sh [napok=7] [--detail]
#   callgraph-arány = callgraph / (callgraph + teljes olvasás [Read / cat] + outline MCP); kis fájl (<=100 sor) nem számít.
#   Célzott (sed -n, head, Read offset) és keresés (grep/rg) külön állapotként jelenik meg.
#   Szekvencia-állapotok sessionönként, időrendben, fájlra normalizálva (projects.json alapján).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
exec python3 - "$ROOT" "$@" <<'PY'
import json, os, sys, time
from collections import Counter, defaultdict
from datetime import datetime, timezone

root = sys.argv[1]
args = [a for a in sys.argv[2:] if not a.startswith("--")]
detail = "--detail" in sys.argv
days = int(args[0]) if args else 7
log = os.path.join(root, ".claude/metrics/callgraph-usage.jsonl")
SMALL = 100      # ennyi sorig a teljes Read megengedett
BIG_OUT = 400    # ennél több soros callgraph-kimenet = túl nagy

if not os.path.isfile(log) or os.path.getsize(log) == 0:
    print(f"Nincs még mérési adat: {log}"); sys.exit(0)
try:
    reg = json.load(open(os.path.expanduser("~/.claude/skills/callgraph/projects.json")))
except Exception:
    reg = {}
roots = {k: (v.get("path") if isinstance(v, dict) else v) for k, v in reg.items()}

def ts(r):
    return datetime.strptime(r["ts"], "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc).timestamp()

def norm(r):
    t = (r.get("target") or "").split("::")[0]
    if not t:
        return None
    if not os.path.isabs(t):
        base = roots.get(r.get("project") or "", root) or root
        t = os.path.join(base, t)
    return os.path.normpath(t)

def short(p):
    for a, b in roots.items():
        if b and p.startswith(b.rstrip("/") + "/"):
            return f"{a}:{p[len(b.rstrip('/'))+1:]}"
    return p

since = time.time() - days * 86400
rows = []
for line in open(log):
    try:
        r = json.loads(line)
        if ts(r) >= since:
            rows.append(r)
    except Exception:
        pass
rows.sort(key=ts)
# régi típusnevek; a szét nem bontott Bash-olvasás (bash_read / bash_php) = bash_unknown — az arányba nem számít
OLD = {"read_php_full": "read_full", "read_php_ranged": "read_ranged", "bash_php": "bash_unknown", "bash_read": "bash_unknown",
       "grep_php": "grep_code", "php_outline": "outline_mcp"}
SKILL_MARK = ("/.claude/skills/callgraph/",)
for r in rows:
    if r["kind"] in OLD:
        r["kind"] = OLD[r["kind"]]
        r.setdefault("lang", "php")
rows = [r for r in rows if not any(m in (r.get("target") or "") for m in SKILL_MARK)]
FULL = ("read_full", "bash_full")
RANGED = ("read_ranged", "bash_ranged")
SEARCH = ("grep_code", "bash_grep")

def small_file(r):
    return r["kind"] in FULL and (r.get("file_lines") or SMALL + 1) <= SMALL

def ratio(xs):
    c = sum(1 for r in xs if r["kind"] == "callgraph")
    o = sum(1 for r in xs if (r["kind"] in FULL and not small_file(r)) or r["kind"] == "outline_mcp")
    return "–" if c + o == 0 else f"{c * 100 // (c + o)}%"

def cgfirst(xs):
    """callgraph-first: a célzott olvasások és fájl-grepek hány %-át előzte meg fájlszintű callgraph ugyanarra a fájlra."""
    t = [r for r in xs if "_cgf" in r]
    if not t:
        return "–"
    return f"{sum(1 for r in t if r['_cgf']) * 100 // len(t)}% (n={len(t)})"

def counts(xs):
    return "  ".join(f"{k}={v}" for k, v in sorted(Counter(r["kind"] for r in xs).items()))

# --- szekvencia-állapotok ---
state = Counter()
examples = defaultdict(list)
insufficient = Counter()      # fájl -> hányszor kellett teljes olvasás callgraph után
insuff_fmt = defaultdict(Counter)
repeated = []
by_session = defaultdict(list)
for r in rows:
    by_session[r.get("session", "")].append(r)

for sess, xs in by_session.items():
    seen_cg = defaultdict(list)   # fájl -> [formátumok]
    cg_paths = []                 # a sessionben callgraph-fal nézett fájlok/mappák
    any_cg = False
    covered = lambda f: bool(f) and f in cg_paths   # csak fájlszintű callgraph számít (a mappás --grep/summary nem)
    for r in xs:
        k, f = r["kind"], norm(r)
        who = r.get("agent", "?")
        if k == "callgraph":
            any_cg = True
            if f:
                seen_cg[f].append(r.get("format", "outline") + ("+grep" if r.get("grep") else ""))
                cg_paths.append(f)
        elif k in FULL:
            if small_file(r):
                state["kis fájl egészben (megengedett)"] += 1
            elif covered(f):
                state["ELÉGTELEN callgraph (utána teljes olvasás)"] += 1
                insufficient[f] += 1
                insuff_fmt[f][(seen_cg.get(f) or ["mappa"])[-1]] += 1
                examples["elegtelen"].append(f"{who}: {short(f)} (előtte: {', '.join(seen_cg.get(f) or ['mappás callgraph'])})")
            else:
                state["KIHAGYOTT callgraph (teljes olvasás előtte nélküle)"] += 1
                examples["kihagyott"].append(f"{who}: {short(f) if f else r.get('target')}")
        elif k in RANGED:
            r["_cgf"] = covered(f)
            if r["_cgf"]:
                state["helyes: callgraph → célzott olvasás (Read offset / sed -n / head)"] += 1
            else:
                state["célzott olvasás callgraph nélkül (sed -n / head / Read offset)"] += 1
                examples["celzott_nelkul"].append(f"{who}: {short(f) if f else r.get('target')}")
        elif k == "bash_grep":
            r["_cgf"] = covered(f)
            if r["_cgf"]:
                state["grep/rg egy fájlra, callgraph után (ugyanarra a fájlra)"] += 1
            else:
                state["grep/rg egy fájlra callgraph nélkül"] += 1
                examples["grep_nelkul"].append(f"{who}: {short(f) if f else r.get('target')}")
        elif k == "grep_code":
            state["Grep tool keresés (minta, nem fájlhoz kötött)"] += 1
        elif k == "bash_unknown":
            state["régi, szét nem bontott Bash-olvasás (az arányba nem számít)"] += 1
        elif k == "outline_mcp":
            state["outline MCP a callgraph helyett"] += 1
        elif k in ("phpast_blocked", "phpast_other"):
            state["tiltott/kivezetett script próbája"] += 1
    for f, fmts in seen_cg.items():
        if len(fmts) > 1:
            state["ismételt callgraph ugyanarra a fájlra"] += 1
            repeated.append(f"{short(f)}: {' → '.join(fmts)}")

cg = [r for r in rows if r["kind"] == "callgraph"]
bad = [r for r in cg if r.get("error_id") or r.get("empty")]
big = [r for r in cg if (r.get("out_lines") or 0) > BIG_OUT]

print(f"Utolsó {days} nap: {len(rows)} esemény")
print(f"  callgraph-first: {cgfirst(rows)}   — célzott olvasás / fájl-grep előtt futott-e callgraph ugyanarra a fájlra")
print(f"  teljes olvasás helyett callgraph: {ratio(rows)}   — callgraph / (callgraph + teljes Read/cat + outline MCP)")
nud = sum(1 for r in rows if r.get("nudged"))
if nud:
    print(f"  callgraph-first jelzés kiküldve: {nud}×")
print(f"  {counts(rows)}")
langs = sorted({r.get("lang") for r in rows if r.get("lang") in ("php", "js")})
if langs:
    print("  nyelvenként: " + "   ".join(f"{l}: arány {ratio([r for r in rows if r.get('lang') in (l, 'dir')])}" for l in langs))
print("\nNaponta:")
for d in sorted({r['ts'][:10] for r in rows}):
    xs = [r for r in rows if r["ts"][:10] == d]
    print(f"  {d}  callgraph-first {cgfirst(xs)}  teljes-helyett {ratio(xs)}  |  {counts(xs)}")
print("\nAgentenként:")
for a in sorted({r.get('agent', '?') for r in rows}):
    xs = [r for r in rows if r.get("agent") == a]
    print(f"  {a}  callgraph-first {cgfirst(xs)}  teljes-helyett {ratio(xs)}  |  {counts(xs)}")
print("\nÁllapotok (sessionönként, időrendben):")
for k, v in state.most_common():
    print(f"  {v:4d}  {k}")
if cg:
    fm = Counter(r.get("format", "outline") for r in cg)
    print("\nCallgraph-formátumok: " + "  ".join(f"{k}={v}" for k, v in fm.most_common()))
    md = Counter(m for r in cg for m in (r.get("mode") or "").split("+") if m)
    if md:
        print("Callgraph-kapcsolók: " + "  ".join(f"--{k}={v}" for k, v in md.most_common()))
    print(f"  --grep: {sum(1 for r in cg if r.get('grep'))}   hibás/üres: {len(bad)}   túl nagy (>{BIG_OUT} sor): {len(big)}")
if bad:
    print("\nHibás / üres callgraph-futások:")
    for r in bad[-10:]:
        print(f"  {r['ts']}  {r.get('agent')}  {r.get('target')}  --format {r.get('format')}"
              + (f"  error-id {r['error_id']}" if r.get("error_id") else "  (üres kimenet)"))
if big:
    print(f"\nTúl nagy kimenetek (>{BIG_OUT} sor):")
    for r in sorted(big, key=lambda r: -(r.get("out_lines") or 0))[:10]:
        print(f"  {r.get('out_lines'):6d} sor  {r.get('target')}  --format {r.get('format')}")
if insufficient:
    print("\nFeedback-kandidátusok (callgraph után mégis teljes olvasás kellett):")
    for f, n in insufficient.most_common(10):
        fm = ", ".join(f"{k}×{v}" for k, v in insuff_fmt[f].items())
        print(f"  {n}×  {short(f)}  (utolsó callgraph-formátum: {fm})")
if detail:
    for title, key in (("Kihagyott callgraph", "kihagyott"), ("Elégtelen callgraph", "elegtelen"), ("Célzott olvasás callgraph nélkül", "celzott_nelkul"), ("grep egy fájlra callgraph nélkül", "grep_nelkul")):
        if examples[key]:
            print(f"\n{title} (utolsó 15):")
            for e in examples[key][-15:]:
                print("  " + e)
    if repeated:
        print("\nIsmételt callgraph (formátum-sorrend):")
        for e in repeated[-15:]:
            print("  " + e)
PY
