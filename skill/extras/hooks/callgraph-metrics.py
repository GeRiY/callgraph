#!/usr/bin/env python3
"""PostToolUse (Read|Grep|Bash|mcp__(php|vue)-outline__.*): méri, hogyan olvassák a kódot az agentek.

Típusok: callgraph | read_full | read_ranged | grep_code | outline_mcp | phpast_other |
         bash_full (cat/less/nl szűrés nélkül) | bash_ranged (sed -n, head/tail, cat|head) | bash_grep (grep/rg/awk)
- relatív útvonal: a parancsban előtte álló `cd` (és a hook cwd-je) szerint abszolút útra feloldva
- a callgraph skill saját fájljai (~/.claude/skills/callgraph) nem számítanak
- callgraph-nál a tool-válaszból: kimenet-sorszám, üres-e, error-id
Napló: <projekt>/.claude/metrics/$CALLGRAPH_METRICS_LOG (alap: callgraph-usage.jsonl).
"""
import json, os, re, shlex, sys, time

LOG_NAME = os.environ.get("CALLGRAPH_METRICS_LOG", "callgraph-usage.jsonl")
RULE = os.environ.get("CALLGRAPH_RULE", ".claude/rules/callgraph.md")
CODE_RE = re.compile(r"\.(php|vue|js|ts|tsx|mjs)$")
SKILL_DIRS = [os.path.realpath(os.path.expanduser(p)) for p in ("~/.claude/skills/callgraph",)]
FULL_READERS = {"cat", "less", "more", "nl", "bat", "tac"}
RANGED_READERS = {"head", "tail", "sed"}
GREP_READERS = {"grep", "egrep", "fgrep", "rg", "ag", "awk"}
MAX_PER_CMD = 5


def lang(p):
    return "php" if p.endswith(".php") else "js"


def is_skill_file(p):
    rp = os.path.realpath(p)
    return any(rp == d or rp.startswith(d + os.sep) for d in SKILL_DIRS)


def resolve(tok, cwd):
    p = os.path.expanduser(tok)
    return os.path.normpath(p if os.path.isabs(p) else os.path.join(cwd, p))


def split_parts(cmd):
    """; && || mentén részekre, a részeken belül | mentén pipeline-szakaszokra (idézőjel-tudatosan)."""
    parts, cur, stage, q, i = [], [], "", None, 0
    while i < len(cmd):
        ch = cmd[i]
        if q:
            stage += ch
            if ch == q:
                q = None
        elif ch in ("'", '"'):
            q = ch; stage += ch
        elif cmd.startswith("&&", i) or cmd.startswith("||", i):
            cur.append(stage); parts.append(cur); cur, stage = [], ""; i += 1
        elif ch == ";":
            cur.append(stage); parts.append(cur); cur, stage = [], ""
        elif ch == "|":
            cur.append(stage); stage = ""
        else:
            stage += ch
        i += 1
    cur.append(stage); parts.append(cur)
    return [[s.strip() for s in p if s.strip()] for p in parts if any(s.strip() for s in p)]


def toks_of(stage):
    try:
        return shlex.split(stage)
    except ValueError:
        return stage.split()


def classify_stage(toks, rest):
    """Az olvasó szakasz fajtája; None = nem olvasás (pl. sed -i)."""
    c = os.path.basename(toks[0])
    if c in GREP_READERS:
        return "bash_grep"
    if c in RANGED_READERS:
        if c == "sed" and any(t == "-i" or t.startswith("-i") for t in toks[1:]):
            return None
        if c == "sed" and "-n" not in toks:
            return "bash_full"
        return "bash_ranged"
    if c in FULL_READERS:
        nxt = [os.path.basename(toks_of(s)[0]) for s in rest if toks_of(s)]
        if any(n in GREP_READERS for n in nxt):
            return "bash_grep"
        if any(n in ("head", "tail", "sed", "wc") for n in nxt):
            return "bash_ranged"
        return "bash_full"
    return None


def bash_events(cmd, cwd):
    out, seen = [], set()
    for part in split_parts(cmd.split("\n")[0]):
        first = toks_of(part[0])
        if first and first[0] == "cd" and len(first) > 1:
            cwd = resolve(first[1], cwd)
            continue
        for idx, stage in enumerate(part):
            toks = toks_of(stage)
            if not toks:
                continue
            kind = classify_stage(toks, part[idx + 1:])
            if not kind:
                continue
            for t in toks[1:]:
                if t.startswith("-") or any(ch in t for ch in "*?$`{}") or not CODE_RE.search(t):
                    continue
                p = resolve(t, cwd)
                if is_skill_file(p) or (kind, p) in seen:
                    continue
                seen.add((kind, p))
                out.append({"kind": kind, "lang": lang(p), "target": p})
            break  # pipeline-onként az első olvasó szakasz számít
    return out[:MAX_PER_CMD]


def callgraph_event(c, resp):
    m = re.search(r"callgraph\.sh\s+([^|;&\n]*)", c)
    a = m.group(1) if m else ""
    rest = re.sub(r"--(project|format|grep|category|depth|lines|strings|const)\s+\S+", "", a)
    rest = re.sub(r"--\S+", "", rest).split()
    target = rest[0] if rest else ""
    o = resp.get("stdout", "") if isinstance(resp, dict) else str(resp or "")
    e = resp.get("stderr", "") if isinstance(resp, dict) else ""
    f = target.split("::")[0]
    g = lambda pat: (re.search(pat, a) or [None, None])[1]
    modes = re.findall(r"--(body|lines|strings|const|depth|full|guess)\b", a)
    err = re.search(r"error-id: ([0-9a-f]+)", (o or "") + "\n" + (e or ""))
    return {"kind": "callgraph", "lang": "js" if re.search(r"\.(vue|js|ts|tsx|mjs)$", f) else "php" if f.endswith(".php") else "dir",
            "target": target, "project": g(r"--project\s+(\S+)"), "format": g(r"--format\s+(\S+)") or "outline",
            "grep": g(r"--grep\s+(\S+)"), "mode": "+".join(modes) or None,
            "out_lines": 0 if not o else len(o.split("\n")), "empty": not (o or "").strip(),
            "error_id": err.group(1) if err else None}


SMALL_FILE = 100
NUDGE_KINDS = ("bash_ranged", "bash_grep", "read_ranged")
PROJECTS_JSON = os.path.expanduser("~/.claude/skills/callgraph/projects.json")


def registry():
    try:
        with open(PROJECTS_JSON) as f:
            reg = json.load(f)
    except Exception:
        return {}
    return {a: os.path.realpath(v.get("path") if isinstance(v, dict) else v) for a, v in reg.items() if v}


def cg_abs(r, cwd):
    """callgraph-esemény céljának abszolút útja (fájl vagy mappa, ::szelektor nélkül)."""
    t = (r.get("target") or "").split("::")[0]
    if not t:
        return None
    if os.path.isabs(os.path.expanduser(t)):
        return os.path.realpath(os.path.expanduser(t))
    root = registry().get(r.get("project") or "") or cwd
    return os.path.realpath(os.path.join(root, t))


def is_covered(path, covered):
    """Csak a fájlszintű callgraph számít (a mappás --grep / summary nem mondja meg a fájl szerkezetét)."""
    return os.path.realpath(path) in covered


def session_state(log_path, session, cwd):
    covered, nudged = [], set()
    if not session:
        return covered, nudged
    try:
        with open(log_path) as fh:
            for line in fh:
                if session not in line:
                    continue
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                if r.get("session") != session:
                    continue
                if r.get("kind") == "callgraph":
                    covered.append(cg_abs(r, cwd))
                if r.get("nudged"):
                    nudged.add(r.get("target"))
    except OSError:
        pass
    return covered, nudged


def cg_command(path):
    rp = os.path.realpath(path)
    best = None
    for alias, root in registry().items():
        if rp.startswith(root + os.sep) and (best is None or len(root) > len(best[1])):
            best = (alias, root)
    if best:
        return f"bash ~/.claude/skills/callgraph/callgraph.sh --project {best[0]} {shlex.quote(os.path.relpath(rp, best[1]))}"
    return f"bash ~/.claude/skills/callgraph/callgraph.sh {shlex.quote(rp)}"


def main():
    try:
        data = json.load(sys.stdin)
    except Exception:
        return
    t, i = data.get("tool_name", ""), data.get("tool_input") or {}
    cwd = data.get("cwd") or os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()
    recs = []
    if t == "Read":
        fp = i.get("file_path") or ""
        if CODE_RE.search(fp) and not is_skill_file(fp):
            recs.append({"kind": "read_ranged" if (i.get("offset") or i.get("limit")) else "read_full", "lang": lang(fp),
                         "target": fp, "offset": i.get("offset"), "limit": i.get("limit")})
    elif t == "Grep":
        g = " ".join(str(i.get(k) or "") for k in ("type", "glob", "path"))
        if re.search(r"php|vue|\bjs\b|\.js|\bts\b|\.ts", g):
            recs.append({"kind": "grep_code", "lang": "php" if "php" in g else "js", "target": i.get("pattern", "")})
    elif t == "Bash":
        c = i.get("command") or ""
        c1 = c.split("\n")[0]   # heredoc / beágyazott szöveg nem hívás
        if re.search(r"(^|[|;&(]\s*|\bbash\s+)\S*callgraph\.sh", c1) or re.search(r"CG=\S*callgraph\.sh", c1):
            recs.append(callgraph_event(c, data.get("tool_response")))
        elif re.search(r"callgraph/(where|deps|compare|git-diff|changed|analyze)\.sh", c1):
            recs.append({"kind": "phpast_other", "target": c[:200]})
        else:
            recs.extend(bash_events(c, cwd))
    elif re.match(r"^mcp__(php|vue)-outline__", t):
        recs.append({"kind": "outline_mcp", "lang": "php" if t.startswith("mcp__php") else "js",
                     "target": i.get("file_path") or i.get("path") or ""})
    if not recs:
        return
    ts = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    session = data.get("session_id", "")
    log_dir = os.path.join(os.environ.get("CLAUDE_PROJECT_DIR") or ".", ".claude", "metrics")
    log_path = os.path.join(log_dir, LOG_NAME)
    covered, nudged = session_state(log_path, session, cwd)
    for r in recs:
        if r["kind"] == "callgraph":
            covered.append(cg_abs(r, cwd))
    nudges = []
    try:
        os.makedirs(log_dir, exist_ok=True)
        with open(log_path, "a") as fh:
            for r in recs:
                if r["kind"] in ("read_full", "read_ranged", "bash_full", "bash_ranged", "bash_grep") and os.path.isfile(r["target"]):
                    try:
                        with open(r["target"], "rb") as tf:
                            r["file_lines"] = sum(1 for _ in tf)
                    except OSError:
                        pass
                # callgraph-first jelzés: célzott olvasás / grep egy nagyobb kódfájlon, amire ebben a sessionben
                # még nem futott callgraph — fájlonként egyszer
                if (r["kind"] in NUDGE_KINDS and (r.get("file_lines") or 0) > SMALL_FILE
                        and not is_covered(r["target"], covered) and r["target"] not in nudged):
                    r["nudged"] = True
                    nudged.add(r["target"])
                    nudges.append(r["target"])
                r.update(ts=ts, session=session, agent=data.get("agent_type") or data.get("agent_id") or "main")
                fh.write(json.dumps(r, ensure_ascii=False) + "\n")
    except OSError:
        return
    if nudges:
        cmds = "; ".join(cg_command(p) for p in nudges[:3])
        print(json.dumps({"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext":
              f"callgraph-first ({RULE}): erre a fájlra ebben a sessionben még nem futott callgraph — a következő lépésnél "
              f"előbb: {cmds} (egy metódus törzse: …::név --body; hívók: <mappa> --grep '<minta>'). Csak jelzés, nem tiltás."}}))
        return
    if any(r["kind"] == "outline_mcp" for r in recs):
        print(json.dumps({"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext":
              f"PHP/JS/Vue-kódhoz a callgraph.sh az első lépés, nem az outline MCP ({RULE}): "
              "bash ~/.claude/skills/callgraph/callgraph.sh --project <alias> <fájl>[::név] [--body]"}}))


main()
