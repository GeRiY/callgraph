#!/usr/bin/env python3
"""PreToolUse (Read|Bash) — nagy PHP/JS/TS/Vue fájl teljes beolvasása helyett callgraph.

Csak a callgraph projects.json-jában regisztrált projektek alatti fájlokra hat. A célzott Read
(offset/limit) és a kis fájl (<= THRESHOLD sor) átmegy; Bash-nél a korlátlan cat/less/nl/bat-ot nézi.
CALLGRAPH_GUARD_MODE=warn → nem tilt, csak figyelmeztet (stderr).
"""
import json, os, re, shlex, sys

THRESHOLD = int(os.environ.get("CALLGRAPH_GUARD_LINES", "100"))
EXTS = (".php", ".vue", ".js", ".ts", ".tsx", ".mjs")
SKILL = os.path.expanduser(os.environ.get("CALLGRAPH_HOME", "~/.claude/skills/callgraph"))
READERS = {"cat", "less", "more", "nl", "bat", "tac"}


def registry():
    try:
        with open(os.path.join(SKILL, "projects.json")) as f:
            reg = json.load(f)
    except Exception:
        return {}
    return {a: os.path.realpath(v.get("path") if isinstance(v, dict) else v) for a, v in reg.items() if v}


def advice(path):
    if not path.endswith(EXTS) or not os.path.isfile(path):
        return None
    try:
        with open(path, "rb") as f:
            lines = sum(1 for _ in f)
    except OSError:
        return None
    if lines <= THRESHOLD:
        return None
    real = os.path.realpath(path)
    best = None
    for alias, root in registry().items():
        if real.startswith(root + os.sep) and (best is None or len(root) > len(best[1])):
            best = (alias, root)
    if not best:
        return None
    rel = os.path.relpath(real, best[1])
    return (f"{rel} ({lines} sor): teljes olvasás helyett előbb callgraph — "
            f"bash {SKILL}/callgraph.sh --project {best[0]} {shlex.quote(rel)} "
            f"(egy metódus/függvény: …::név, törzs: …::név --body, sáv: --lines n-m). "
            f"Utána csak célzott Read (offset/limit). Hiba esetén: bash {SKILL}/feedback.sh add <error_id> \"<…>\".")


def emit(reason):
    if os.environ.get("CALLGRAPH_GUARD_MODE", "deny") == "warn":
        sys.stderr.write("[callgraph-guard] " + reason + "\n")
    else:
        print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse",
                                                 "permissionDecision": "deny",
                                                 "permissionDecisionReason": reason}}))
    sys.exit(0)


REDIRECT = re.compile(r"^(\d*|&)(>>?|>\|)")


def read_targets(args):
    """A ténylegesen olvasott fájl-argumentumok: a kimeneti átirányítás célja (`> f`, `>> f`, `2>f`) és a heredoc
    (`<<EOF`, `<<-EOF`, `<<<`) nem olvasás — `cat >> f <<EOF` hozzáfűzés, nem teljes olvasás."""
    out, skip = [], False
    for t in args:
        if skip:
            skip = False
            continue
        if t.startswith("<<"):
            skip = t in ("<<", "<<-", "<<<")  # a jelölő / szöveg külön tokenben jön
            continue
        m = REDIRECT.match(t)
        if m:
            skip = m.end() == len(t)  # `>> f`: a cél a következő token; `>>f`: ugyanebben
            continue
        if t.startswith("-") or t == "<":  # `cat < f`: a f olvasás, maga a `<` nem fájl
            continue
        out.append(t)
    return out


def main():
    try:
        data = json.load(sys.stdin)
    except Exception:
        sys.exit(0)
    tool, ti = data.get("tool_name", ""), data.get("tool_input") or {}
    if tool == "Read":
        if not (ti.get("offset") or ti.get("limit")):
            r = advice(ti.get("file_path") or "")
            if r:
                emit(r)
    elif tool == "Bash":
        cmd = (ti.get("command") or "").split("\n")[0]
        for part in re.split(r";|&&|\|\|", cmd):
            stages = [s.strip() for s in part.split("|") if s.strip()]
            if len(stages) != 1:
                continue  # pipeline → már szűkített
            try:
                toks = shlex.split(stages[0])
            except ValueError:
                continue
            if toks and os.path.basename(toks[0]) in READERS:
                for t in read_targets(toks[1:]):
                    p = t if os.path.isabs(t) else os.path.join(os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd(), t)
                    r = advice(p)
                    if r:
                        emit(r)
    sys.exit(0)


main()
