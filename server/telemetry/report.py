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


if __name__ == "__main__":
    main()
