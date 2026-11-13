"""Daily summary of the callgraph.eu page views and the telemetry batches (text, for the team chat).

docker exec callgraph-telemetry python report.py [days]
"""

import sqlite3
import sys
from datetime import date, timedelta

DB = "/data/telemetry.sqlite"


def main() -> None:
    days = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    since = (date.today() - timedelta(days=days)).isoformat()
    con = sqlite3.connect(DB)
    q = lambda sql: con.execute(sql, (since,)).fetchall()  # noqa: E731
    views = q("SELECT COALESCE(SUM(count),0) FROM hits WHERE event='view' AND day>=?")[0][0]
    downloads = q("SELECT COALESCE(SUM(count),0) FROM hits WHERE event='download' AND day>=?")[0][0]
    print(f"callgraph.eu since {since}: {views} page views, {downloads} download clicks")
    print("pages: " + ", ".join(f"{p} {n}" for p, n in q(
        "SELECT page, SUM(count) n FROM hits WHERE event='view' AND day>=? GROUP BY page ORDER BY n DESC LIMIT 8")))
    print("from: " + ", ".join(f"{r or '(direct)'} {n}" for r, n in q(
        "SELECT referrer, SUM(count) n FROM hits WHERE event='view' AND day>=? GROUP BY referrer ORDER BY n DESC LIMIT 8")))
    tel = q("SELECT COUNT(*), COUNT(DISTINCT platform) FROM days WHERE received>=?")[0]
    print(f"telemetry: {tel[0]} daily batches received ({tel[1]} platforms)")
    usage(con, since)
    graph(con, since)


def graph(con: sqlite3.Connection, since: str) -> None:
    """Az opt-in telemetria parancs-gráfja: a leggyakoribb átmenetek és a sessionök száma."""
    try:
        rows = con.execute(
            "SELECT src, dst, err, rel, SUM(count) n FROM edges WHERE day>=? GROUP BY src, dst, err, rel "
            "ORDER BY n DESC LIMIT 10", (since,)).fetchall()
        sessions = con.execute("SELECT COALESCE(SUM(count),0) FROM session_stats WHERE day>=?", (since,)).fetchone()[0]
    except sqlite3.Error:
        return
    if not rows and not sessions:
        return
    print(f"command graph (opt-in telemetry): {sessions} sessions; top transitions:")
    for src, dst, err, rel, n in rows:
        print(f"  {src}{'!' if err else ''} → {dst} ({rel}): {n}")
    try:
        uses = con.execute(
            "SELECT src, dst, f, SUM(count) n FROM uses WHERE day>=? AND f!='-' GROUP BY src, dst, f "
            "ORDER BY src, dst, n DESC", (since,)).fetchall()
    except sqlite3.Error:
        return
    if uses:
        print("target from previous output (file band):")
        by_pair: dict = {}
        for src, dst, f, n in uses:
            by_pair.setdefault((src, dst), []).append(f"{f} {n}")
        for (src, dst), parts in sorted(by_pair.items(), key=lambda kv: -sum(int(p.split()[1]) for p in kv[1]))[:10]:
            print(f"  {src} → {dst}: " + ", ".join(parts))


def usage(con: sqlite3.Connection, since: str) -> None:
    """Anonymous counters (check.callgraph.eu, update.callgraph.eu): active installs, new installs, updates."""
    try:
        con.execute("SELECT 1 FROM checks LIMIT 1")
    except sqlite3.Error:
        return
    today = date.today()
    week = (today - timedelta(days=today.weekday())).isoformat()
    month = today.replace(day=1).isoformat()
    one = lambda sql, *a: con.execute(sql, a).fetchone()[0] or 0  # noqa: E731
    print("active installs (daily check; one per install and day):")
    for day, n, new, mcp in con.execute(
        "SELECT day, SUM(count), SUM(CASE WHEN n=1 THEN count END), SUM(CASE WHEN via='mcp' THEN count END) "
        "FROM checks WHERE day>=? GROUP BY day ORDER BY day", (since,)):
        print(f"  {day}: {n} active, {new or 0} new, {mcp or 0} via MCP")
    print(f"  this week (since {week}): {one('SELECT SUM(count) FROM checks WHERE w=1 AND day>=?', week)} active"
          f" · this month: {one('SELECT SUM(count) FROM checks WHERE m=1 AND day>=?', month)} active")
    versions = con.execute("SELECT version, SUM(count) n FROM checks WHERE day>=? GROUP BY version ORDER BY n DESC LIMIT 6",
                           (since,)).fetchall()
    if versions:
        print("  versions: " + ", ".join(f"{v} {n}" for v, n in versions))
    platforms = con.execute("SELECT platform, SUM(count) n FROM checks WHERE day>=? GROUP BY platform ORDER BY n DESC",
                            (since,)).fetchall()
    if platforms:
        print("  platforms: " + ", ".join(f"{p} {n}" for p, n in platforms))
    rows = dict(con.execute("SELECT event, SUM(count) FROM updates WHERE day>=? GROUP BY event", (since,)).fetchall())
    print(f"updates: {rows.get('check', 0)} update checks, {rows.get('update', 0)} update downloads, "
          f"{rows.get('install', 0)} installer downloads")


if __name__ == "__main__":
    main()
