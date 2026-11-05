#!/usr/bin/env python3
"""Több token_bench-eredmény (verziók) összevetése feladattípusonként: token, helyesség, idő.

Használat:
  python3 -I tools/bench_compare.py v0.2=eredmeny-v020.json v0.3=eredmeny-v030.json [--html riport.html]
      [--rtt 3.0] [--tok-per-s 5000]

Idők (ms / s):
  - parancs-idő (mért): a baseline eszközei (grep, Read), a callgraph meleg és hideg cache-sel — `--runs` medián;
  - agent-idő (BECSLÉS): lépés × modell-körút (`--rtt`, alap 3 s) + beolvasott token / feldolgozási sebesség
    (`--tok-per-s`, alap 5000 token/s) + a parancsok saját ideje. A valós falióra modell- és terhelésfüggő;
    a becslés arra jó, hogy a × arány mögött álló nagyságrend (mp) látsszon.
Számot a repóba ne írj: a kimenetet a repón kívülre mentsd.
"""
import argparse
import html
import json
import statistics as st

TYPES = ["outline", "api", "body", "callers", "callers_d2", "strings", "const"]
NAMES = {"outline": "outline", "api": "publikus API", "body": "metódus-törzs", "callers": "ki hívja (1 szint)",
         "callers_d2": "ki hívja (2 szint)", "strings": "string-keresés", "const": "konstans-keresés", "ALL": "összes"}


def agent_seconds(tokens, steps, tool_s, rtt, tps):
    return steps * rtt + tokens / tps + tool_s


def summarize(tasks, rtt, tps):
    def med(f):
        return st.median(f(t) for t in tasks)

    def cg_tok(t):
        return t["effective"]["tok_b4"]

    def cg_tool_s(t, key):
        s = t["time_s"][key]
        return s if not t["effective"]["fallback"] else s + t["time_s"]["baseline"]

    return {
        "n": len(tasks),
        "ok": sum(1 for t in tasks if t["correctness"].get("ok")),
        "x": med(lambda t: t["x_eff_smart_tok_b4"]),
        "tok_smart": med(lambda t: t["baseline_smart"]["tok_b4"]),
        "tok_cg": med(cg_tok),
        "steps_smart": med(lambda t: t["baseline_smart"]["steps"]),
        "steps_cg": med(lambda t: t["effective"]["steps"]),
        "ms_base": med(lambda t: t["time_s"]["baseline"] * 1000),
        "ms_warm": med(lambda t: t["time_s"]["cg_warm"] * 1000),
        "ms_cold": med(lambda t: t["time_s"]["cg_cold"] * 1000),
        "agent_smart_s": med(lambda t: agent_seconds(t["baseline_smart"]["tok_b4"], t["baseline_smart"]["steps"],
                                                     t["time_s"]["baseline"], rtt, tps)),
        "agent_cg_s": med(lambda t: agent_seconds(cg_tok(t), t["effective"]["steps"], cg_tool_s(t, "cg_warm"),
                                                  rtt, tps)),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("runs", nargs="+", help="címke=eredmény.json")
    ap.add_argument("--html")
    ap.add_argument("--rtt", type=float, default=3.0, help="egy modell-körút (eszközhívás) ideje, mp")
    ap.add_argument("--tok-per-s", type=float, default=5000.0, help="beolvasott token feldolgozása, token/mp")
    a = ap.parse_args()
    data = {}
    for spec in a.runs:
        label, path = spec.split("=", 1)
        data[label] = json.load(open(path))["tasks"]
    labels = list(data)
    rows = {}
    for ty in TYPES + ["ALL"]:
        rows[ty] = {lb: summarize(data[lb] if ty == "ALL" else [t for t in data[lb] if t["type"] == ty],
                                  a.rtt, a.tok_per_s) for lb in labels}
    hdr = f"{'típus':<20}{'verzió':<8}{'helyes':>8}{'×':>7}{'token okos→cg':>16}{'lépés':>9}" \
          f"{'parancs ms grep/cg meleg/hideg':>32}{'agent mp okos→cg (becsl.)':>28}"
    print(hdr)
    for ty, r in rows.items():
        for lb in labels:
            s = r[lb]
            print(f"{NAMES[ty]:<20}{lb:<8}{s['ok']:>4}/{s['n']:<3}{s['x']:>7.2f}"
                  f"{s['tok_smart']:>8.0f}→{s['tok_cg']:<7.0f}{s['steps_smart']:>4.1f}→{s['steps_cg']:<4.1f}"
                  f"{s['ms_base']:>12.0f} /{s['ms_warm']:>7.0f} /{s['ms_cold']:>7.0f}"
                  f"{s['agent_smart_s']:>14.1f} →{s['agent_cg_s']:>6.1f} s")
    if a.html:
        write_html(a.html, rows, labels, a)


def write_html(path, rows, labels, a):
    def tr(ty):
        cells = []
        for i, lb in enumerate(labels):
            s = rows[ty][lb]
            first = f"<td rowspan={len(labels)}>{html.escape(NAMES[ty])}</td>" if i == 0 else ""
            cells.append(
                f"<tr>{first}<td>{html.escape(lb)}</td><td>{s['ok']}/{s['n']}</td><td>{s['x']:.2f}×</td>"
                f"<td>{s['tok_smart']:.0f} → {s['tok_cg']:.0f}</td><td>{s['steps_smart']:.1f} → {s['steps_cg']:.1f}</td>"
                f"<td>{s['ms_base']:.0f} ms</td><td>{s['ms_warm']:.0f} ms / {s['ms_cold']:.0f} ms</td>"
                f"<td>{s['agent_smart_s']:.1f} s → <b>{s['agent_cg_s']:.1f} s</b></td></tr>")
        return "".join(cells)

    body = "".join(tr(ty) for ty in TYPES + ["ALL"])
    doc = f"""<!DOCTYPE html><html lang="hu"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>callgraph — mérés-összevetés</title>
<style>body{{font:14px/1.45 system-ui,sans-serif;margin:12px}}table{{border-collapse:collapse}}
th,td{{border-bottom:1px solid #ccd;padding:5px 8px;text-align:right;white-space:nowrap}}
th:first-child,td:first-child{{text-align:left}}.wrap{{overflow-x:auto}}</style></head><body>
<h1>Mérés-összevetés: {" / ".join(html.escape(lb) for lb in labels)}</h1>
<p>Medián feladattípusonként. <b>×</b> = tényleges token-arány az okos baseline-hoz képest. <b>Parancs ms</b>: mért futásidő
(grep/Read út; callgraph meleg / hideg cache). <b>Agent mp</b>: BECSLÉS = lépés × {a.rtt:g} s modell-körút + token / {a.tok_per_s:g}
token/s + a parancsok ideje.</p>
<div class="wrap"><table><tr><th>típus</th><th>verzió</th><th>helyes</th><th>×</th><th>token okos → cg</th>
<th>lépés okos → cg</th><th>grep/Read</th><th>callgraph meleg / hideg</th><th>agent (becslés)</th></tr>{body}</table></div>
</body></html>"""
    open(path, "w").write(doc)


if __name__ == "__main__":
    main()
