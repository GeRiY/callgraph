#!/usr/bin/env python3
"""token_bench.py — callgraph vs. Read/Grep: token-, lépés- és időmérés valósághű agent-feladatokon.

Csak stdlib. A korpuszokat csak olvassa (grep + fájlolvasás + a callgraph bináris); semmit nem futtat belőlük.
A callgraph a <gyökér>/.callgraph/ cache-t írja, ezért a korpuszból érdemes munkamásolatot adni.

Használat:
  python3 -I tools/token_bench.py --bin <callgraph> \
      --corpus koel-php:php:/út/koel:app \
      --corpus koel-ts:ts:/út/koel:resources/assets/js \
      --corpus axum:rust:/út/axum:. \
      --out results.json [--html report.html] [--runs 5]

A --corpus formátuma: <név>:<nyelv>:<gyökér>:<almappa>; nyelv: php | rust | ts | swift | scss.
Részletek: docs/TOKEN-BENCH.md.
"""
import argparse
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
import time

# --------------------------------------------------------------------------- nyelvek

EXTS = {
    "php": (".php",),
    "rust": (".rs",),
    "ts": (".ts", ".tsx", ".js", ".jsx", ".vue", ".mjs", ".cjs"),
    "swift": (".swift",),
    "scss": (".scss",),
}

DEF_RE = {
    "php": re.compile(r"^\s*(?:(?:public|protected|private|static|final|abstract)\s+)*function\s+&?(\w+)\s*\("),
    "rust": re.compile(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?"
                       r"(?:extern\s+\"[^\"]*\"\s+)?fn\s+(\w+)"),
    "ts": re.compile(r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*(\w+)\s*[<(]"
                     r"|^\s*(?:export\s+)?(?:const|let)\s+(\w+)\s*(?::[^=]+)?=\s*(?:async\s*)?(?:\([^)]*\)|\w+)\s*(?::[^=]+)?=>"
                     r"|^\s+(?:(?:public|private|protected|static|async|get|set)\s+)*(?!if\b|for\b|while\b|switch\b|catch\b|return\b)(\w+)\s*\([^)]*\)\s*(?::[^{]+)?\{\s*$"
                     r"|^\s+(\w+)\s*:\s*(?:async\s*)?(?:function\b|\([^)]*\)\s*(?::[^=]+)?=>|\w+\s*=>)"),
    "swift": re.compile(r"^\s*(?:@\w+\s+)*(?:(?:public|private|fileprivate|internal|open|static|final|override|mutating|class)\s+)*func\s+(\w+)"),
    "scss": re.compile(r"^\s*@(?:mixin|function)\s+([\w-]+)"),
}

CONTAINER_RE = {
    "php": re.compile(r"^\s*(?:(?:abstract|final|readonly)\s+)*(?:class|trait|interface|enum)\s+(\w+)"),
    "rust": re.compile(r"^\s*impl(?:<[^>]*>)?\s+(?:[\w:<>, ]+\s+for\s+)?([A-Z]\w*)"),
    "swift": re.compile(r"^\s*(?:(?:public|private|final|open)\s+)*(?:class|struct|enum|actor|extension|protocol)\s+(\w+)"),
}

# Gyakori nevek: ezekre egy agent sem grepel értelmesen (túl sok név-ütközés), a feladatválasztás kihagyja őket.
STOP = set("""new get set has put add run main test default from into build render handle make call apply
fmt clone drop next len is_empty push pop insert remove clear map filter find parse write read open close
init setup update create delete store show index destroy boot register up down toString valueOf constructor
data computed mounted watch emit value with when then each all first last count keys values merge""".split())

TEST_PATH = re.compile(r"(^|/)(tests?|__tests__|spec|benches|examples)(/|$)|\.spec\.|\.test\.|_test\.|/testutil")

BRACE_LANGS ={"php", "rust", "ts", "swift", "scss"}

# --------------------------------------------------------------------------- token-becslők

WORD_RE = re.compile(r"[A-Za-z]+|\d+|[^\sA-Za-z\d]")


def est_tokens(text):
    """Két közelítés (a Claude tokenizere nem érhető el helyben): bájt/4 és szó/írásjel-alapú."""
    b = len(text.encode("utf-8"))
    return {"bytes": b, "tok_b4": math.ceil(b / 4), "tok_words": len(WORD_RE.findall(text))}


# --------------------------------------------------------------------------- korpusz

class Corpus:
    def __init__(self, spec):
        name, lang, root, sub = spec.split(":", 3)
        if lang not in EXTS:
            sys.exit(f"ismeretlen nyelv: {lang}")
        self.name, self.lang, self.root, self.sub = name, lang, os.path.abspath(root), sub
        self.files = []  # gyökérhez relatív utak
        base = os.path.join(self.root, sub)
        for dp, dns, fns in os.walk(base):
            dns[:] = sorted(d for d in dns if d not in (".git", "node_modules", "vendor", "target", ".callgraph"))
            for fn in sorted(fns):
                if fn.endswith(EXTS[lang]):
                    self.files.append(os.path.relpath(os.path.join(dp, fn), self.root))
        self._cache = {}

    def lines(self, rel):
        if rel not in self._cache:
            with open(os.path.join(self.root, rel), encoding="utf-8", errors="replace") as fh:
                self._cache[rel] = fh.read().split("\n")
        return self._cache[rel]

    def text(self, rel):
        return "\n".join(self.lines(rel))


# --------------------------------------------------------------------------- forrás-elemzés (regex + zárójel-párosítás)

def strip_code(line):
    """String- és megjegyzés-tartalom kivágása a zárójel-számláshoz (közelítő)."""
    line = re.sub(r'"(?:\\.|[^"\\])*"', '""', line)
    line = re.sub(r"'(?:\\.|[^'\\])*'", "''", line)
    line = re.sub(r"`(?:\\.|[^`\\])*`", "``", line)
    line = re.sub(r"//.*$", "", line)
    line = re.sub(r"(?<!\w)#(?!\[).*$", "", line)
    return line


QUOTES = {"rust": '"', "swift": '"', "php": "\"'", "ts": "\"'`", "scss": "\"'"}


def code_lines(corpus, rel):
    """A fájl sorainak kód-része: a megjegyzések (`//`, `/* … */` több soron át is, PHP `#`) kivágva, a stringek maradnak.
    Fájl-szintű állapotgép (a többsoros string és a `/*! … */` doc-blokk is helyes); az eredmény a korpuszon cache-elve."""
    cache = corpus.__dict__.setdefault("_code_lines", {})
    if rel in cache:
        return cache[rel]
    text, lang = corpus.text(rel), corpus.lang
    quotes = QUOTES.get(lang, '"')
    out, line = [], []
    state, quote, i, n = "code", None, 0, len(text)
    while i < n:
        c = text[i]
        if c == "\n":
            out.append("".join(line))
            line = []
            if state == "line":
                state = "code"
            i += 1
            continue
        if state == "code":
            if text.startswith("//", i) or (c == "#" and lang == "php" and not text.startswith("#[", i)):
                state = "line"
            elif text.startswith("/*", i):
                state, i = "block", i + 2
                continue
            elif c in quotes:
                state, quote = "str", c
                line.append(c)
            else:
                line.append(c)
        elif state == "str":
            line.append(c)
            if c == "\\" and i + 1 < n and text[i + 1] != "\n":
                line.append(text[i + 1])
                i += 2
                continue
            if c == quote:
                state = "code"
        elif state == "block" and text.startswith("*/", i):
            state, i = "code", i + 2
            continue
        i += 1
    out.append("".join(line))
    cache[rel] = out
    return out


def code_hits(corpus, t, hits):
    """A grep-találatok közül azok, ahol a keresett név / szöveg a kódban van, nem csak megjegyzésben."""
    ty = t["type"]
    if ty in ("callers", "callers_d2"):
        rx = re.compile(call_pattern(t["name"]))
        keep = lambda code: rx.search(code)
    elif ty == "strings":
        keep = lambda code: t["literal"] in code
    else:
        rx = re.compile(r"\b" + re.escape(t["name"]) + r"\b")
        keep = lambda code: rx.search(code)
    def code(h):
        lines = code_lines(corpus, h[0])
        return lines[h[1]] if h[1] < len(lines) else h[2]

    return [h for h in hits if keep(code(h))]


def block_end(lines, start):
    """A start sorban kezdődő {…} blokk utolsó sora (0-bázisú), vagy None."""
    depth, seen = 0, False
    for i in range(start, min(len(lines), start + 3000)):
        s = strip_code(lines[i])
        for ch in s:
            if ch == "{":
                depth += 1
                seen = True
            elif ch == "}":
                depth -= 1
                if seen and depth == 0:
                    return i
        if not seen and i > start + 8:
            return None
        if not seen and s.rstrip().endswith(";"):
            return None  # absztrakt / interfész-deklaráció
    return None


def definitions(corpus, rel):
    """[(név, container, kezdősor0, zárósor0)] a fájlban."""
    out, lang = [], corpus.lang
    lines = corpus.lines(rel)
    cont, crex = None, CONTAINER_RE.get(lang)
    for i, ln in enumerate(lines):
        if crex:
            m = crex.match(ln)
            if m:
                cont = m.group(1)
        m = DEF_RE[lang].match(ln)
        if not m:
            continue
        name = next(g for g in m.groups() if g)
        end = block_end(lines, i) if lang in BRACE_LANGS else None
        out.append((name, cont, i, end))
    return out


def enclosing_def(defs, line0):
    best = None
    for d in defs:
        if d[2] <= line0 and (d[3] is None or d[3] >= line0):
            if best is None or d[2] > best[2]:
                best = d
    return best


# --------------------------------------------------------------------------- baseline-eszközök (Read / Grep szimuláció)

def read_tool(lines, start0=0, end0=None):
    """A Read eszköz kimenete: cat -n formátum (6 széles sorszám + tab)."""
    end0 = len(lines) - 1 if end0 is None else min(end0, len(lines) - 1)
    return "".join(f"{i + 1:>6}\t{lines[i]}\n" for i in range(max(0, start0), end0 + 1))


def grep(corpus, pattern, fixed=False, word=False, context=0):
    args = ["grep", "-rn", "-I"]
    args += ["-F"] if fixed else ["-E"]
    if word:
        args.append("-w")
    if context:
        args += ["-C", str(context)]
    for e in EXTS[corpus.lang]:
        args.append(f"--include=*{e}")
    args += ["-e", pattern, corpus.sub]
    t0 = time.perf_counter()
    p = subprocess.run(args, cwd=corpus.root, capture_output=True, text=True, errors="replace")
    dt = time.perf_counter() - t0
    hits = []
    if not context:
        for ln in p.stdout.splitlines():
            m = re.match(r"^(.*?):(\d+):(.*)$", ln)
            if m:
                hits.append((os.path.normpath(m.group(1)), int(m.group(2)) - 1, m.group(3)))
    return p.stdout, hits, dt


def windows_read(corpus, hits_by_file, radius=30):
    """Okos baseline: fájlonként egy Read, a találatok ±radius sora (az átfedő ablakok összevonva)."""
    out, steps = [], 0
    for rel, lns in sorted(hits_by_file.items()):
        lines = corpus.lines(rel)
        spans = []
        for l0 in sorted(set(lns)):
            a, b = max(0, l0 - radius), min(len(lines) - 1, l0 + radius)
            if spans and a <= spans[-1][1] + 1:
                spans[-1][1] = max(spans[-1][1], b)
            else:
                spans.append([a, b])
        # egy Read hívás offset/limit-tel: a legelső ablaktól a legutolsóig, ha közel vannak; különben ablakonként
        for a, b in spans:
            out.append(read_tool(lines, a, b))
            steps += 1
    return "".join(out), steps


def full_reads(corpus, files):
    return "".join(read_tool(corpus.lines(f)) for f in sorted(files)), len(files)


# --------------------------------------------------------------------------- callgraph

class Callgraph:
    def __init__(self, binary):
        self.bin = os.path.abspath(binary)
        self.env = dict(os.environ, CALLGRAPH_LANG="en", CALLGRAPH_UPDATE_CHECK="0", NO_COLOR="1")

    def argv(self, corpus, args):
        return [self.bin, "show", "--project", corpus.root] + args

    def run(self, corpus, args):
        t0 = time.perf_counter()
        p = subprocess.run(self.argv(corpus, args), cwd=corpus.root, capture_output=True, text=True,
                           errors="replace", env=self.env)
        return p.stdout + (p.stderr if p.returncode else ""), time.perf_counter() - t0, p.returncode

    def clear_cache(self, corpus):
        shutil.rmtree(os.path.join(corpus.root, ".callgraph"), ignore_errors=True)


def cmd_str(args):
    return "callgraph show " + " ".join(a if re.fullmatch(r"[\w./:@-]+", a) else "'" + a.replace("'", "'\\''") + "'"
                                        for a in args)


# --------------------------------------------------------------------------- feladatválasztás

def pick_tasks(corpus, per_type):
    """Determinisztikus feladatválasztás a valós kódból."""
    tasks = []
    lang = corpus.lang
    alldefs = {f: definitions(corpus, f) for f in corpus.files}
    small = len(corpus.files) < 30  # pl. a golden-minták: lazább küszöbök
    lo = 30 if small else 120
    # a kérdések termékkódra vonatkoznak: teszt-, spec- és deklarációs fájl nem lehet outline/api/body célpont
    prod = [f for f in corpus.files if not TEST_PATH.search(f) and not f.endswith(".d.ts")]
    sizes = sorted(((len(corpus.lines(f)), f) for f in prod), reverse=True)
    big = [(n, f) for n, f in sizes if lo <= n <= 2000 and len(alldefs[f]) >= (1 if lang == "scss" else 3)]

    # outline + api: a 2000 sornál rövidebb fájlok közül a nagyobbak (a Read alapkorlátja 2000 sor)
    if big:
        picks = [big[len(big) // 10], big[len(big) // 3]] if len(big) > 3 else big[:1]
        for n, f in picks[:per_type]:
            tasks.append({"type": "outline", "file": f, "q": f"Mi van a {f} fájlban (szerkezet, metódusok, hívások)?"})
        for n, f in big[len(big) // 5: len(big) // 5 + 1]:
            if lang in ("php", "rust", "ts", "swift"):
                tasks.append({"type": "api", "file": f, "q": f"Mi a {f} publikus API-ja?"})

    # body: 15–90 soros függvény egy ≥250 soros fájlban
    cands = []
    for n, f in big:
        if n < (60 if small else 250):
            continue
        for name, cont, s, e in alldefs[f]:
            if e is not None and (8 if small else 15) <= e - s <= 90 and name not in STOP:
                cands.append((n, e - s, f, name, cont, s, e))
    cands.sort(key=lambda c: (-c[0], -c[1], c[2], c[3]))
    seen = set()
    for c in cands:
        if c[2] in seen:
            continue
        seen.add(c[2])
        tasks.append({"type": "body", "file": c[2], "name": c[3], "container": c[4], "start": c[5], "end": c[6],
                      "q": f"Mit csinál {c[4] + '::' if c[4] else ''}{c[3]} ({c[2]})?"})
        if len([t for t in tasks if t["type"] == "body"]) >= per_type:
            break

    # callers: egyszer definiált, nem-triviális név, 4–20 hívó fájllal
    if lang != "scss":
        defcount, defsite = {}, {}
        for f, ds in alldefs.items():
            for name, cont, s, e in ds:
                defcount[name] = defcount.get(name, 0) + 1
                defsite[name] = (f, cont, s)
        call_files = {}
        call_re = re.compile(r"(?<![\w$])([A-Za-z_]\w{4,})\s*(?:::<[^>]*>)?\s*\(")
        for f in corpus.files:
            dlines = {d[2] for d in alldefs[f]}
            for i, ln in enumerate(corpus.lines(f)):
                if i in dlines:
                    continue
                for m in call_re.finditer(ln):
                    nm = m.group(1)
                    if defcount.get(nm) == 1:
                        call_files.setdefault(nm, set()).add(f)
        lo_c = 2 if small else 4
        ranked = sorted(((len(v), k) for k, v in call_files.items() if lo_c <= len(v) <= 20 and k not in STOP),
                        key=lambda x: (-x[0], x[1]))
        got = 0
        for cnt, nm in ranked:
            f, cont, s = defsite[nm]
            tasks.append({"type": "callers", "name": nm, "container": cont, "file": f, "start": s,
                          "q": f"Ki hívja {cont + '::' if cont else ''}{nm}-t?"})
            got += 1
            if got >= per_type:
                break
        # depth 2: közepes név (3–8 hívó fájl), hogy a második szint is kezelhető legyen
        for cnt, nm in sorted(((len(v), k) for k, v in call_files.items()
                               if (2 if small else 3) <= len(v) <= 8 and k not in STOP),
                              key=lambda x: (-x[0], x[1])):
            if any(t.get("name") == nm for t in tasks):
                continue
            f, cont, s = defsite[nm]
            tasks.append({"type": "callers_d2", "name": nm, "container": cont, "file": f, "start": s,
                          "q": f"Hatáselemzés: ki hívja {cont + '::' if cont else ''}{nm}-t, és azokat ki hívja (2 szint)?"})
            break

    # strings: ≥ 12 karakteres, szóközt tartalmazó literál 2–6 fájlban
    lit_re = re.compile(r"""(['"])([A-Za-z][A-Za-z0-9 ,:-]{10,48}[A-Za-z.!])\1""")
    lit_files = {}
    for f in prod:  # a feladat valós hibaüzenetre / kulcsra kérdez, nem teszt-fixture-re
        for ln in corpus.lines(f):
            if ln.lstrip().startswith(("//", "*", "#", "/*")):
                continue
            for m in lit_re.finditer(ln):
                s = m.group(2)
                if " " in s:
                    lit_files.setdefault(s, set()).add(f)
    lits = sorted(((len(v), k) for k, v in lit_files.items() if 2 <= len(v) <= 6), key=lambda x: (-x[0], x[1]))
    for cnt, s in lits[:1]:
        tasks.append({"type": "strings", "literal": s, "q": f"Hol szerepel a(z) „{s}” szöveg?"})

    # const: PHP osztálykonstans / Rust const
    if lang in ("php", "rust"):
        cre = re.compile(r"^\s*(?:(?:public|protected|private|final)\s+)*const\s+(?:\w+\s+)?([A-Z][A-Z0-9_]{3,})\s*[:=]") \
            if lang == "php" else re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?const\s+([A-Z][A-Z0-9_]{3,})\s*:")
        cdef = {}
        for f in corpus.files:
            for ln in corpus.lines(f):
                m = cre.match(ln)
                if m:
                    cdef.setdefault(m.group(1), []).append(f)
        uses = []
        for nm, fs in cdef.items():
            if len(fs) != 1:
                continue
            _, hits, _ = grep(corpus, nm, fixed=True, word=True)
            files = {h[0] for h in hits}
            if 3 <= len(files) <= 12:
                uses.append((len(files), nm))
        uses.sort(key=lambda x: (-x[0], x[1]))
        for cnt, nm in uses[:1]:
            tasks.append({"type": "const", "name": nm, "q": f"Hol definiálják és használják a {nm} konstanst?"})
    return tasks


# --------------------------------------------------------------------------- stratégiák

def baseline(corpus, t):
    """(naiv, okos): mindkettő {text, steps, cmds}; az idő külön mérve."""
    lang = corpus.lang
    ty = t["type"]
    if ty in ("outline", "api"):
        txt = read_tool(corpus.lines(t["file"]))
        r = {"text": txt, "steps": 1, "cmds": [f"Read {t['file']}"]}
        return r, dict(r)
    if ty == "body":
        lines = corpus.lines(t["file"])
        naive = {"text": read_tool(lines), "steps": 1, "cmds": [f"Read {t['file']}"]}
        gout, _, _ = grep_file(corpus, t["file"], def_pattern(lang, t["name"]))
        # okos: grep a definícióra, majd Read offset/limit — a pontos sávval (alsó korlát: az agent nem tudja előre a végét)
        smart = {"text": gout + read_tool(lines, t["start"], t["end"]), "steps": 2,
                 "cmds": [f"grep -n '{def_pattern(lang, t['name'])}' {t['file']}",
                          f"Read {t['file']} offset={t['start'] + 1} limit={t['end'] - t['start'] + 1}"]}
        return naive, smart
    if ty in ("callers", "callers_d2"):
        pat = call_pattern(t["name"])
        gout, hits, _ = grep(corpus, pat)
        hits = [h for h in hits if not is_def_hit(corpus, h)]
        by_file = {}
        for h in hits:
            by_file.setdefault(h[0], []).append(h[1])
        n_txt, n_steps = full_reads(corpus, by_file.keys())
        s_txt, s_steps = windows_read(corpus, by_file)
        naive = {"text": gout + n_txt, "steps": 1 + n_steps, "cmds": [f"grep -rnE '{pat}' {corpus.sub}"]}
        smart = {"text": gout + s_txt, "steps": 1 + s_steps, "cmds": [f"grep -rnE '{pat}' {corpus.sub}"]}
        if ty == "callers_d2":
            # 2. szint: az 1. szint hívó függvényeinek nevére újabb grep + olvasás
            level1 = set()
            for h in hits:
                d = enclosing_def(definitions(corpus, h[0]), h[1])
                if d and d[0] != t["name"]:
                    level1.add(d[0])
            read_full, read_win = set(by_file), {k: set(v) for k, v in by_file.items()}
            level2 = set()
            for nm in sorted(level1):
                p2 = call_pattern(nm)
                g2, h2, _ = grep(corpus, p2)
                h2 = [h for h in h2 if not is_def_hit(corpus, h)]
                naive["text"] += g2
                smart["text"] += g2
                naive["steps"] += 1
                smart["steps"] += 1
                naive["cmds"].append(f"grep -rnE '{p2}' {corpus.sub}")
                smart["cmds"].append(f"grep -rnE '{p2}' {corpus.sub}")
                new_full = {h[0] for h in h2} - read_full
                txt, st = full_reads(corpus, new_full)
                naive["text"] += txt
                naive["steps"] += st
                read_full |= new_full
                new_win = {}
                for h in h2:
                    if h[1] not in read_win.get(h[0], set()):
                        new_win.setdefault(h[0], []).append(h[1])
                txt, st = windows_read(corpus, new_win)
                smart["text"] += txt
                smart["steps"] += st
                for k, v in new_win.items():
                    read_win.setdefault(k, set()).update(v)
                for h in h2:
                    d = enclosing_def(definitions(corpus, h[0]), h[1])
                    if d and d[0] != nm:
                        level2.add(d[0])
            t["_level1_grep"] = sorted(level1)
            t["_level2_grep"] = sorted(level2 - level1)
        t["_grep_hits"] = hits
        t["_grep_hits_code"] = code_hits(corpus, t, hits)
        return naive, smart
    if ty == "strings":
        gout, hits, _ = grep(corpus, t["literal"], fixed=True)
        n_txt, n_steps = full_reads(corpus, {h[0] for h in hits})
        cout, _, _ = grep(corpus, t["literal"], fixed=True, context=3)
        t["_grep_hits"] = hits
        t["_grep_hits_code"] = code_hits(corpus, t, hits)
        return ({"text": gout + n_txt, "steps": 1 + n_steps, "cmds": [f"grep -rnF '{t['literal']}' {corpus.sub}"]},
                {"text": cout, "steps": 1, "cmds": [f"grep -rnF -C3 '{t['literal']}' {corpus.sub}"]})
    if ty == "const":
        gout, hits, _ = grep(corpus, t["name"], fixed=True, word=True)
        n_txt, n_steps = full_reads(corpus, {h[0] for h in hits})
        cout, _, _ = grep(corpus, t["name"], fixed=True, word=True, context=3)
        t["_grep_hits"] = hits
        t["_grep_hits_code"] = code_hits(corpus, t, hits)
        return ({"text": gout + n_txt, "steps": 1 + n_steps, "cmds": [f"grep -rnw '{t['name']}' {corpus.sub}"]},
                {"text": cout, "steps": 1, "cmds": [f"grep -rnw -C3 '{t['name']}' {corpus.sub}"]})
    raise ValueError(ty)


def def_pattern(lang, name):
    return {"php": f"function {name}\\b", "rust": f"fn {name}\\b", "swift": f"func {name}\\b",
            "scss": f"@(mixin|function) {name}\\b"}.get(lang, f"\\b{name}\\b")


def call_pattern(name):
    return f"\\b{name}\\s*\\("


def grep_file(corpus, rel, pattern):
    t0 = time.perf_counter()
    p = subprocess.run(["grep", "-nE", pattern, rel], cwd=corpus.root, capture_output=True, text=True,
                       errors="replace")
    return p.stdout, None, time.perf_counter() - t0


def is_def_hit(corpus, h):
    return bool(DEF_RE[corpus.lang].match(h[2]))


def cg_args(corpus, t, qualified=False):
    ty = t["type"]
    if ty == "outline":
        return [t["file"]]
    if ty == "api":
        return [t["file"], "--format", "api"]
    if ty == "body":
        return [f"{t['file']}::{t['name']}", "--body"]
    if ty in ("callers", "callers_d2"):
        # elsődleges: név szerinti minta (a grep-pel összevethető); qualified=True: a SKILL.md stílusa, Típus::név
        if qualified and t.get("container"):
            pat = f"{t['container']}::{t['name']}$"
        else:
            pat = f"(^|::|\\.|->){t['name']}$"
        args = [corpus.sub, "--grep", pat]
        if ty == "callers_d2":
            args += ["--depth", "2"]
        return args
    if ty == "strings":
        return [corpus.sub, "--strings", t["literal"]]  # részsztring, kis-/nagybetű-független
    if ty == "const":
        return [corpus.sub, "--const", t["name"]]
    raise ValueError(ty)


# --------------------------------------------------------------------------- helyesség

CG_HIT_RE = re.compile(r"^\s+(\S+\.\w+)\s+(\S+?)\(?\)?\s+L(\d+)-(\d+)")


def cg_hits(corpus, out):
    """A --grep kimenet sorai: (relatív fájl, metódus, kezdő, záró) — szintenként."""
    levels, cur, cur_file = {}, 0, None

    def rel(f):
        return os.path.normpath(os.path.relpath(f, corpus.root) if os.path.isabs(f) else f)

    for ln in out.splitlines():
        m = re.match(r"^== (?:level|szint) (\d+)", ln)
        if m:
            cur = int(m.group(1))
            continue
        m = re.match(r"^(?:FILE|FÁJL)\s+(\S+)", ln)
        if m:
            cur_file = rel(m.group(1))
            continue
        # --depth formátum: "  <fájl>  <Típus::metódus()>  L<a>-<b>"
        m = re.match(r"^\s+(\S+\.\w+)\s+(\S+?)(?:\(\))?\s+L(\d+)-(\d+)", ln)
        if m and cur:
            levels.setdefault(cur, []).append((rel(m.group(1)), m.group(2), int(m.group(3)), int(m.group(4))))
            continue
        # outline formátum (--grep mélység nélkül): "  <metódus>()  L<a>-<b>"
        m = re.match(r"^  (\S+?)\(\)\s+L(\d+)-(\d+)", ln)
        if m and cur_file and not cur:
            levels.setdefault(1, []).append((cur_file, m.group(1), int(m.group(2)), int(m.group(3))))
            continue
        # 400 sor fölötti tömör forma: "<fájl>:<a>-<b>  <metódus>" vagy hasonló
        m = re.match(r"^\s*(\S+\.\w+):L?(\d+)(?:-(\d+))?\s+(\S+)", ln)
        if m and not cur:
            levels.setdefault(1, []).append((rel(m.group(1)), m.group(4), int(m.group(2)),
                                             int(m.group(3) or m.group(2))))
    return levels


def cg_files_any(corpus, out):
    """Fájlutak bármilyen callgraph-kimenetből (strings / const / outline-fejlécek)."""
    files = set()
    # az első „root: /abs” sor maga nem találat (utána az utak ehhez relatívak)
    out = "\n".join(ln for ln in out.splitlines() if not re.match(r"^(?:root|gyökér): ", ln))
    for tok in re.findall(r"(?:^|\s|\()((?:/|\.?[\w-]+/)[\w./@-]+\.\w+)(?::\d+)?", out, re.M):
        f = os.path.relpath(tok, corpus.root) if os.path.isabs(tok) else tok
        if os.path.exists(os.path.join(corpus.root, f)):
            files.add(os.path.normpath(f))
    return files


def correctness(corpus, t, out):
    ty = t["type"]
    if ty in ("outline", "api"):
        defs = definitions(corpus, t["file"])
        if ty == "api":  # csak a publikus definíciók számítanak
            lines = corpus.lines(t["file"])
            pub = {"php": lambda s: not re.search(r"\b(private|protected)\b", s),
                   "rust": lambda s: re.match(r"\s*pub\b(?!\s*\((?:crate|super|self)\))", s),
                   "swift": lambda s: re.search(r"\b(public|open)\b", s),
                   "ts": lambda s: re.match(r"\s*export\b", s)}[corpus.lang]
            defs = [d for d in defs if pub(lines[d[2]])]
        names = {d[0] for d in defs}
        found = {n for n in names if re.search(r"\b" + re.escape(n) + r"\b", out)}
        miss = sorted(names - found)
        rec = len(found) / len(names) if names else 1.0
        return {"ok": rec >= 0.9, "def_recall": round(rec, 3), "missing": miss[:15],
                "note": f"definíciók: {len(found)}/{len(names)} látszik" + (f"; hiányzik: {', '.join(miss[:6])}" if miss else "")}
    if ty == "body":
        # a függvény KÓD-sorai (a megjegyzés-sorok nem kötelezőek: a tömör --body elhagyja őket);
        # a kódsor megjegyzés nélküli részének kell szerepelnie a kimenetben
        code = code_lines(corpus, t["file"])
        want = [code[i].strip() for i in range(t["start"], t["end"] + 1) if code[i].strip()]
        got = sum(1 for w in want if w in out)
        rec = got / len(want) if want else 1.0
        return {"ok": rec >= 0.98, "line_recall": round(rec, 3),
                "note": f"a törzs {got}/{len(want)} nem üres sora benne van (független zárójel-párosítással mérve)"}
    if ty in ("callers", "callers_d2"):
        levels = cg_hits(corpus, out)
        l1 = levels.get(1, [])
        ghits = t.get("_grep_hits_code", t["_grep_hits"])
        gfiles = {h[0] for h in ghits}
        cfiles = {h[0] for h in l1}
        both = gfiles & cfiles
        # a grep-találat sorát lefedi-e egy callgraph-sáv (ugyanabban a fájlban)
        covered = sum(1 for h in ghits
                      if any(c[0] == h[0] and c[2] - 1 <= h[1] <= c[3] - 1 for c in l1))
        res = {"grep_files": len(gfiles), "grep_files_raw": len({h[0] for h in t["_grep_hits"]}),
               "cg_files": len(cfiles), "both": len(both),
               "grep_only": sorted(gfiles - cfiles)[:20], "cg_only": sorted(cfiles - gfiles)[:20],
               "hit_coverage": round(covered / len(ghits), 3) if ghits else None,
               "file_recall_vs_grep": round(len(both) / len(gfiles), 3) if gfiles else None,
               "file_precision_vs_grep": round(len(both) / len(cfiles), 3) if cfiles else None}
        if ty == "callers_d2":
            # név-szintű összevetés (a grep-oldal a legközelebbi megelőző definíció: közelítés, lezárások / beágyazott fn-ek tévesztenek)
            short = lambda s: s.split("::")[-1].split(".")[-1]
            g1, g2 = set(t.get("_level1_grep", [])), set(t.get("_level2_grep", []))
            c1 = {short(h[1]) for h in levels.get(1, [])} - {t["name"]}
            c2 = {short(h[1]) for h in levels.get(2, [])}
            res["level1_names"] = {"grep": sorted(g1), "cg": sorted(c1), "both": len(g1 & c1)}
            res["level2_names"] = {"grep": sorted(g2)[:40], "cg": sorted(c2)[:40], "both": len(g2 & c2),
                                   "grep_n": len(g2), "cg_n": len(c2)}
            res["unresolved_skipped"] = sum(int(m) for m in re.findall(r"\(\+(\d+) (?:unresolved|feloldatlan)", out))
        res["ok"] = bool(gfiles) and res["file_recall_vs_grep"] is not None and res["file_recall_vs_grep"] >= 0.9
        res["note"] = (f"fájlszint: grep {len(gfiles)}, callgraph {len(cfiles)}, közös {len(both)}; "
                       f"grep-sorok lefedve: {covered}/{len(ghits)}")
        if ty == "callers_d2":
            l2 = res["level2_names"]
            res["note"] += (f"; 2. szint nevei: grep {l2['grep_n']}, callgraph {l2['cg_n']}, közös {l2['both']}"
                            f" (+{res['unresolved_skipped']} feloldatlan kihagyva)")
        return res
    if ty in ("strings", "const"):
        gfiles = {h[0] for h in t.get("_grep_hits_code", t["_grep_hits"])}
        cfiles = cg_files_any(corpus, out)
        both = gfiles & cfiles
        rec = len(both) / len(gfiles) if gfiles else 0.0
        return {"ok": bool(gfiles) and rec >= 0.9, "file_recall_vs_grep": round(rec, 3), "grep_files": len(gfiles),
                "grep_files_raw": len({h[0] for h in t["_grep_hits"]}), "cg_files": len(cfiles), "both": len(both),
                "grep_only": sorted(gfiles - cfiles)[:20], "cg_only": sorted(cfiles - gfiles)[:20],
                "note": f"fájlszint: grep {len(gfiles)}, callgraph {len(cfiles)}, közös {len(both)}"}
    return {"ok": None, "note": ""}


# --------------------------------------------------------------------------- időmérés

def median_time(fn, runs):
    ts = []
    for _ in range(runs):
        ts.append(fn())
    return statistics.median(ts)


def time_baseline(corpus, t, runs):
    """A baseline parancsainak tényleges futásideje (grep-ek + fájlolvasások), a modell körútja NÉLKÜL."""
    def once():
        t0 = time.perf_counter()
        corpus._cache.clear()
        baseline(corpus, dict(t))
        return time.perf_counter() - t0
    return median_time(once, runs)


# --------------------------------------------------------------------------- futtatás

def run(args):
    cg = Callgraph(args.bin)
    results = []
    for spec in args.corpus:
        corpus = Corpus(spec)
        print(f"[{corpus.name}] {len(corpus.files)} fájl", file=sys.stderr)
        tasks = pick_tasks(corpus, args.per_type)
        for t in tasks:
            naive, smart = baseline(corpus, t)
            cargs = cg_args(corpus, t)
            cg.clear_cache(corpus)
            out, _, rc = cg.run(corpus, cargs)  # bemelegítés (és a cold-cache utáni első futás)
            cold = median_time(lambda: (cg.clear_cache(corpus), cg.run(corpus, cargs)[1])[1], args.runs)
            cg.run(corpus, cargs)
            warm = median_time(lambda: cg.run(corpus, cargs)[1], args.runs)
            bt = time_baseline(corpus, t, args.runs)
            corr = correctness(corpus, t, out)
            rec = {
                "corpus": corpus.name, "lang": corpus.lang, "type": t["type"], "question": t["q"],
                "target": t.get("file") or t.get("name") or t.get("literal"),
                "baseline_naive": {**est_tokens(naive["text"]), "steps": naive["steps"], "cmds": naive["cmds"][:12]},
                "baseline_smart": {**est_tokens(smart["text"]), "steps": smart["steps"], "cmds": smart["cmds"][:12]},
                "callgraph": {**est_tokens(out), "steps": 1, "cmd": cmd_str(cargs), "rc": rc,
                              "head": "\n".join(out.splitlines()[:25])},
                "time_s": {"baseline": round(bt, 4), "cg_cold": round(cold, 4), "cg_warm": round(warm, 4)},
                "correctness": corr,
            }
            if t["type"] in ("callers", "callers_d2") and t.get("container"):
                qargs = cg_args(corpus, t, qualified=True)
                qout, qt, _ = cg.run(corpus, qargs)
                qc = correctness(corpus, t, qout)
                rec["callgraph_qualified"] = {**est_tokens(qout), "cmd": cmd_str(qargs), "time_s": round(qt, 4),
                                              "correctness": {k: v for k, v in qc.items() if k != "grep_only"}}
            # a „hogyan hívják?”: a grep-ablakok (okos baseline) megmutatják a hívás sorát; ha a callgraph-kimenetben nincs
            # hívássor („L<n> │ …”), az agentnek még egy `--grep … --body` kell — ennek a költsége is a callgraph-oldalra kerül
            follow = {"tok_b4": 0, "tok_words": 0, "steps": 0}
            if t["type"] in ("callers", "callers_d2") and "│" not in out:
                fargs = [a for a in cargs if a not in ("--depth", "2")] + ["--body"]
                fout, _, _ = cg.run(corpus, fargs)
                follow = {**est_tokens(fout), "steps": 1}
                rec["callgraph_followup"] = {**follow, "cmd": cmd_str(fargs)}
            # tényleges költség: ha a callgraph-válasz a kritérium szerint hiányos, az agent a grep-es úton is végigmegy
            ok = bool(corr.get("ok"))
            rec["effective"] = {
                "fallback": not ok,
                "tok_b4": rec["callgraph"]["tok_b4"] + follow["tok_b4"] + (0 if ok else rec["baseline_smart"]["tok_b4"]),
                "tok_words": rec["callgraph"]["tok_words"] + follow["tok_words"]
                + (0 if ok else rec["baseline_smart"]["tok_words"]),
                "steps": 1 + follow["steps"] + (0 if ok else rec["baseline_smart"]["steps"]),
            }
            for k in ("tok_b4", "tok_words"):
                c = max(1, rec["callgraph"][k])
                rec[f"x_naive_{k}"] = round(rec["baseline_naive"][k] / c, 2)
                rec[f"x_smart_{k}"] = round(rec["baseline_smart"][k] / c, 2)
                c = max(1, rec["effective"][k])
                rec[f"x_eff_naive_{k}"] = round(rec["baseline_naive"][k] / c, 2)
                rec[f"x_eff_smart_{k}"] = round(rec["baseline_smart"][k] / c, 2)
            results.append(rec)
            print(f"  {t['type']:<10} {str(rec['target'])[:50]:<50} naiv {rec['baseline_naive']['tok_b4']:>7} "
                  f"okos {rec['baseline_smart']['tok_b4']:>7} cg {rec['callgraph']['tok_b4']:>6} "
                  f"×{rec['x_smart_tok_b4']:<7} eff×{rec['x_eff_smart_tok_b4']:<6} {corr.get('note', '')[:70]}",
                  file=sys.stderr)
        cg.clear_cache(corpus)
    return results


def aggregate(results, key):
    groups = {}
    for r in results:
        groups.setdefault(r[key], []).append(r)
    out = {}
    for g, rs in sorted(groups.items()):
        med = lambda f: round(statistics.median(f(r) for r in rs), 2)
        out[g] = {
            "n": len(rs),
            "x_naive": med(lambda r: r["x_naive_tok_b4"]),
            "x_smart": med(lambda r: r["x_smart_tok_b4"]),
            "x_smart_words": med(lambda r: r["x_smart_tok_words"]),
            "x_eff_naive": med(lambda r: r["x_eff_naive_tok_b4"]),
            "x_eff_smart": med(lambda r: r["x_eff_smart_tok_b4"]),
            "x_eff_smart_words": med(lambda r: r["x_eff_smart_tok_words"]),
            "steps_eff": med(lambda r: r["effective"]["steps"]),
            "sum_naive": sum(r["baseline_naive"]["tok_b4"] for r in rs),
            "sum_smart": sum(r["baseline_smart"]["tok_b4"] for r in rs),
            "sum_eff": sum(r["effective"]["tok_b4"] for r in rs),
            "tok_naive": med(lambda r: r["baseline_naive"]["tok_b4"]),
            "tok_smart": med(lambda r: r["baseline_smart"]["tok_b4"]),
            "tok_cg": med(lambda r: r["callgraph"]["tok_b4"]),
            "steps_naive": med(lambda r: r["baseline_naive"]["steps"]),
            "steps_smart": med(lambda r: r["baseline_smart"]["steps"]),
            "steps_cg": 1,
            "t_base_ms": med(lambda r: r["time_s"]["baseline"] * 1000),
            "t_cold_ms": med(lambda r: r["time_s"]["cg_cold"] * 1000),
            "t_warm_ms": med(lambda r: r["time_s"]["cg_warm"] * 1000),
            "ok": sum(1 for r in rs if r["correctness"].get("ok")),
            "cg_bigger": sum(1 for r in rs if r["x_smart_tok_b4"] < 1),
        }
    return out


def print_table(results, aggs):
    print(f"{'típus/nyelv':<14}{'n':>3}{'naiv tok':>10}{'okos tok':>10}{'cg tok':>8}{'×naiv':>8}{'×okos':>8}"
          f"{'×eff':>8}{'lépés n/o/cg':>14}{'ms base':>9}{'ms cold':>9}{'ms warm':>9}{'ok':>4}")
    for title, agg in aggs:
        print(f"-- {title}")
        for g, a in agg.items():
            print(f"{g:<14}{a['n']:>3}{a['tok_naive']:>10}{a['tok_smart']:>10}{a['tok_cg']:>8}{a['x_naive']:>8}"
                  f"{a['x_smart']:>8}{a['x_eff_smart']:>8}"
                  f"{str(a['steps_naive']) + '/' + str(a['steps_smart']) + '/' + str(a['steps_eff']):>14}"
                  f"{a['t_base_ms']:>9}{a['t_cold_ms']:>9}{a['t_warm_ms']:>9}{a['ok']:>4}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bin", help="a callgraph bináris")
    ap.add_argument("--corpus", action="append", help="<név>:<nyelv>:<gyökér>:<almappa>")
    ap.add_argument("--out", default="token-bench.json")
    ap.add_argument("--html", help="önálló HTML-riport ide")
    ap.add_argument("--report-text", help="JSON: {headline, method, limitations, notes{<korpusz>/<típus>/<cél>: …}} — a riport kézi szövegei (HTML)")
    ap.add_argument("--render", help="mérés nélkül: egy korábbi eredmény-JSON-ból csak a HTML-riport")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--per-type", type=int, default=2, help="feladat típusonként és korpuszonként (outline/body/callers)")
    args = ap.parse_args()
    extra = None
    if args.report_text:
        with open(args.report_text, encoding="utf-8") as fh:
            extra = json.load(fh)
    if args.render:
        with open(args.render, encoding="utf-8") as fh:
            doc = json.load(fh)
        write_html(doc, args.html or "token-bench-report.html", extra)
        return
    if not args.bin or not args.corpus:
        ap.error("--bin és legalább egy --corpus kell (vagy --render)")
    results = run(args)
    aggs = [("feladattípus", aggregate(results, "type")), ("nyelv", aggregate(results, "lang")),
            ("összes", aggregate([dict(r, all="összes") for r in results], "all"))]
    doc = {"estimator": "tok_b4 = ceil(bájt/4); tok_words = [A-Za-z]+ | \\d+ | írásjel darabszám — mindkettő a Claude "
                        "tokenizerének közelítése",
           "runs": args.runs, "binary": args.bin, "corpora": args.corpus,
           "aggregate": {t: a for t, a in aggs}, "tasks": results}
    with open(args.out, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, ensure_ascii=False, indent=1)
    print_table(results, aggs)
    if args.html:
        write_html(doc, args.html, extra)
        print(f"HTML: {args.html}", file=sys.stderr)


CSS_VARS = (":root{--bg:#f7fafe;--bg-2:#e8f1fe;--fg:#0e1a32;--card:#fff;--muted:#e8eff9;--muted-fg:#4c586f;"
            "--accent:#dce9fc;--accent-fg:#0f2352;--primary:#194cb1;--primary-2:#4934a3;--primary-fg:#f8fafd;"
            "--border:#d0d8e5;--danger:#d40c1a;--success:#009b44;--warning:#fe9a00;--radius:10px} "
            "@media (prefers-color-scheme:dark){:root{--bg:#0c152a;--bg-2:#070c1e;--fg:#edf2f9;--card:#151f38;"
            "--muted:#1f2b44;--muted-fg:#aab9ce;--accent:#233459;--accent-fg:#f1f6fc;--primary:#63b9fe;"
            "--primary-2:#9ba1fb;--primary-fg:#081431;--border:#a6c0e538;--danger:#ff6467;--success:#00c950;"
            "--warning:#ffb900}}")

TYPE_HU = {"outline": "Fájl-szerkezet (outline)", "api": "Publikus API", "body": "Metódus-törzs",
           "callers": "Ki hívja (1 szint)", "callers_d2": "Hatáselemzés (2 szint)", "strings": "String-keresés",
           "const": "Konstans-keresés"}


def write_html(doc, path, extra=None):
    """Önálló HTML-riport (inline CSS, script nélkül). Az `extra` (dict): headline, limitations — kézi szöveg HTML-ben."""
    import html as H
    e = H.escape
    extra = extra or {}
    agg_t, agg_l, agg_all = doc["aggregate"]["feladattípus"], doc["aggregate"]["nyelv"], doc["aggregate"]["összes"]

    def fx(x):
        return (f"{x:.1f}×" if x >= 1 else f"{x:.2f}×").replace(".", ",")

    def num(v):
        return f"{v:,.0f}".replace(",", " ")

    def agg_rows(agg, label):
        rows = []
        for g, a in agg.items():
            rows.append(f"<tr><td>{e(label(g))}</td><td class=n>{a['n']}</td><td class=n>{num(a['tok_naive'])}</td>"
                        f"<td class=n>{num(a['tok_smart'])}</td><td class=n>{num(a['tok_cg'])}</td>"
                        f"<td class=n>{fx(a['x_eff_naive'])}</td><td class=n>{fx(a['x_smart'])}</td>"
                        f"<td class=n><b>{fx(a['x_eff_smart'])}</b></td>"
                        f"<td class=n>{a['steps_naive']:g} / {a['steps_smart']:g} / {a['steps_eff']:g}</td>"
                        f"<td class=n>{a['t_base_ms']:.0f}</td><td class=n>{a['t_cold_ms']:.0f}</td>"
                        f"<td class=n>{a['t_warm_ms']:.0f}</td><td class=n>{a['ok']}/{a['n']}</td></tr>")
        return "".join(rows)

    head = ("<tr><th>Csoport</th><th class=n>n</th><th class=n>naiv tok</th><th class=n>okos tok</th>"
            "<th class=n>callgraph tok</th><th class=n>× naiv, tényleges</th><th class=n>× okos, nyers</th><th class=n>× okos, tényleges</th>"
            "<th class=n>lépés naiv/okos/cg</th><th class=n>ms baseline</th><th class=n>ms cg hideg</th>"
            "<th class=n>ms cg meleg</th><th class=n>helyes</th></tr>")

    # oszlopdiagram: log-skálás vízszintes sávok típusonként
    mx = max([max(a["x_eff_naive"], a["x_eff_smart"]) for a in agg_t.values()] + [2])
    lmx = math.log10(mx)

    def bar(x, cls):
        w = 0 if x <= 1 else 100 * math.log10(x) / lmx
        return (f"<div class='bar {cls}'><span style='width:{max(w, 0.8):.1f}%'></span><em>{fx(x)}</em></div>")

    chart = "".join(f"<div class=crow><div class=clab>{e(TYPE_HU.get(g, g))}</div><div class=cbars>"
                    f"{bar(a['x_eff_smart'], 's')}{bar(a['x_eff_naive'], 'v')}</div></div>" for g, a in agg_t.items())

    qrows = []
    for r in doc["tasks"]:
        q = r.get("callgraph_qualified")
        if not q:
            continue
        qc = q["correctness"]
        qrows.append(f"<tr><td>{e(r['corpus'])} · {e(TYPE_HU.get(r['type'], r['type']))}</td><td><code>{e(q['cmd'])}</code></td>"
                     f"<td class=n>{qc.get('both', 0)}/{qc.get('grep_files', 0)}</td><td class=n>{num(q['tok_b4'])}</td>"
                     f"<td><code>{e(r['callgraph']['cmd'])}</code></td><td class=n>{r['correctness'].get('both', 0)}/"
                     f"{r['correctness'].get('grep_files', 0)}</td><td class=n>{num(r['callgraph']['tok_b4'])}</td></tr>")
    qtable = ("<div class=tw><table><tr><th>Feladat</th><th>Típus::név minta (SKILL.md-stílus)</th><th class=n>hívó fájl</th>"
              "<th class=n>tok</th><th>Név szerinti minta (a mérés alapja)</th><th class=n>hívó fájl</th><th class=n>tok</th></tr>"
              + "".join(qrows) + "</table></div>") if qrows else ""

    trs = []
    for r in doc["tasks"]:
        c = r["correctness"]
        mark = ("<i class='dot ok'></i>" if c.get("ok") else "<i class='dot bad'></i>" if c.get("ok") is False
                else "<i class='dot na'></i>")
        x = r["x_eff_smart_tok_b4"]
        warn = " <i class='dot warn'></i>" if x < 1 else ""
        fb = " <span class=q>(+grep-tartalék)</span>" if r["effective"]["fallback"] else ""
        note = extra.get("notes", {}).get(f"{r['corpus']}/{r['type']}/{r['target']}", "")
        note = f"<br><span class=q>{e(note)}</span>" if note else ""
        trs.append(
            f"<tr><td><b>{e(r['corpus'])}</b> · {e(TYPE_HU.get(r['type'], r['type']))}<br>"
            f"<span class=q>{e(r['question'])}</span><br><code>{e(r['callgraph']['cmd'])}</code></td>"
            f"<td class=n>{num(r['baseline_naive']['tok_b4'])}</td><td class=n>{num(r['baseline_smart']['tok_b4'])}</td>"
            f"<td class=n>{num(r['callgraph']['tok_b4'])}{fb}</td>"
            f"<td class=n>{fx(r['x_eff_naive_tok_b4'])}<br><b>{fx(x)}</b>{warn}</td>"
            f"<td class=n>{r['baseline_naive']['steps']} / {r['baseline_smart']['steps']} / {r['effective']['steps']}</td>"
            f"<td class=n>{r['time_s']['baseline'] * 1000:.0f} / {r['time_s']['cg_cold'] * 1000:.0f} / "
            f"{r['time_s']['cg_warm'] * 1000:.0f}</td><td>{mark} {e(c.get('note', ''))}"
            f"{note}"
            f"</td></tr>")

    a = agg_all["összes"]
    css = CSS_VARS + """
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.55 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}
main{max-width:1180px;margin:0 auto;padding:28px 20px 60px}
h1{font-size:28px;margin:0 0 6px}h2{font-size:20px;margin:34px 0 10px}h3{font-size:16px;margin:18px 0 6px}
.sub{color:var(--muted-fg);margin:0 0 18px}
.card{background:var(--card);border:1px solid var(--border);border-radius:var(--radius);padding:16px 18px;margin:12px 0}
.hero{background:linear-gradient(135deg,var(--bg-2),var(--card));}
.kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(170px,1fr));gap:12px;margin:14px 0}
.kpi{background:var(--card);border:1px solid var(--border);border-radius:var(--radius);padding:12px 14px}
.kpi b{display:block;font-size:24px;color:var(--fg)}.kpi span{color:var(--muted-fg);font-size:13px}
.kpi{border-top:3px solid var(--primary)}
table{border-collapse:collapse;width:100%;font-size:13.5px;background:var(--card)}
th,td{border-bottom:1px solid var(--border);padding:7px 8px;text-align:left;vertical-align:top}
th{background:var(--muted);color:var(--fg);font-weight:600;position:sticky;top:0}
td.n,th.n{text-align:right;white-space:nowrap;font-variant-numeric:tabular-nums}
.tw{overflow-x:auto;border:1px solid var(--border);border-radius:var(--radius)}
code{font:12px/1.4 ui-monospace,SFMono-Regular,Menlo,monospace;background:var(--muted);color:var(--fg);padding:1px 4px;border-radius:4px;word-break:break-all}
.q{color:var(--muted-fg);font-size:12.5px}
.dot{display:inline-block;width:9px;height:9px;border-radius:50%;vertical-align:middle}
.dot.ok{background:var(--success)}.dot.bad{background:var(--danger)}.dot.warn{background:var(--warning)}.dot.na{background:var(--border)}
.crow{display:grid;grid-template-columns:210px 1fr;gap:12px;align-items:center;margin:8px 0}
.clab{color:var(--fg);font-size:14px}
.bar{position:relative;height:18px;margin:3px 0;background:var(--muted);border-radius:5px}
.bar span{display:block;height:100%;border-radius:5px}
.bar.s span{background:var(--primary)}.bar.v span{background:var(--primary-2);opacity:.55}
.bar em{position:absolute;right:6px;top:0;font-style:normal;font-size:12px;line-height:18px;color:var(--fg)}
.legend{color:var(--muted-fg);font-size:13px}.legend i{display:inline-block;width:12px;height:10px;border-radius:3px;margin:0 4px 0 12px}
ul{margin:6px 0 6px 20px;padding:0}li{margin:4px 0}
"""
    body = f"""<main>
<h1>callgraph vs. Read/Grep — token-mérés</h1>
<p class=sub>{e(str(len(doc['tasks'])))} valós agent-feladat, {e(str(len(agg_l)))} nyelv, korpuszok: {e(', '.join(c.split(':')[0] for c in doc['corpora']))}. Becslő: bájt/4 (és szó/írásjel-alapú kontroll). Idő: {doc['runs']} futás mediánja.</p>
<div class="card hero">{extra.get('headline', '')}</div>
<div class=kpis>
<div class=kpi><b>{fx(a['x_eff_smart'])}</b><span>medián token-csökkenés az okos baseline-hoz képest (tényleges, a hibás válaszok grep-tartalékával)</span></div>
<div class=kpi><b>{fx(a['x_eff_naive'])}</b><span>medián token-csökkenés a naiv baseline-hoz képest</span></div>
<div class=kpi><b>{fx(a['sum_smart'] / max(1, a['sum_eff']))}</b><span>összesített tokenarány (Σ okos baseline / Σ callgraph tényleges)</span></div>
<div class=kpi><b>{a['t_warm_ms']:.0f} / {a['t_cold_ms']:.0f} ms</b><span>callgraph medián futásidő (meleg / hideg cache)</span></div>
<div class=kpi><b>{a['ok']}/{a['n']}</b><span>feladat, ahol a callgraph-válasz a mért kritérium szerint teljes</span></div>
</div>
<h2>Módszer röviden</h2><div class=card>{extra.get('method', '')}</div>
<h2>Token-megtakarítás feladattípusonként</h2>
<div class=card><p class=legend>log-skála, medián:<i style="background:var(--primary)"></i>okos baseline / callgraph (tényleges)<i style="background:var(--primary-2);opacity:.55"></i>naiv baseline / callgraph (tényleges)</p>{chart}</div>
<h2>Összesítés</h2>
<h3>Feladattípus szerint</h3><div class=tw><table>{head}{agg_rows(agg_t, lambda g: TYPE_HU.get(g, g))}</table></div>
<h3>Nyelv szerint</h3><div class=tw><table>{head}{agg_rows(agg_l, lambda g: g)}{agg_rows(agg_all, lambda g: 'Összes')}</table></div>
<h2>Feladatonként</h2>
<p class=legend>× sorai: naiv / okos baseline-hoz képest, a tényleges költséggel (ha a callgraph-válasz hiányos, az okos baseline költsége hozzáadódik: „+grep-tartalék”); <i class="dot warn" style="margin:0 4px"></i>= a callgraph-út drágább, mint az okos baseline. Idő: baseline / cg hideg / cg meleg (ms, a parancsok saját ideje, a modell körútja nélkül).</p>
<div class=tw><table><tr><th>Feladat és callgraph-parancs</th><th class=n>naiv tok</th><th class=n>okos tok</th><th class=n>cg tok</th><th class=n>×</th><th class=n>lépés n/o/cg</th><th class=n>ms</th><th>Helyesség</th></tr>{''.join(trs)}</table></div>
<h2>Hívó-keresés: Típus::név vs. név szerinti minta</h2>
<p class=legend>Ugyanaz a „ki hívja” kérdés kétféle --grep mintával; „hívó fájl” = a grep-pel talált hívó fájlokból hányat ad vissza.</p>
{qtable}
<h2>Korlátok — ahol nem segít, vagy ahol a mérés torzít</h2><div class=card>{extra.get('limitations', '')}</div>
<p class=q>Előállította: tools/token_bench.py · nyers adatok: a JSON-eredményfájl.</p>
</main>"""
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(f"<!DOCTYPE html><html lang=hu><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>"
                 f"<title>callgraph token-mérés</title><style>{css}</style></head><body>{body}</body></html>")


if __name__ == "__main__":
    main()
