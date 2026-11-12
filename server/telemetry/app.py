"""callgraph opt-in telemetry receiver, callgraph.eu page-view counter, and the anonymous usage / update
counters (check.callgraph.eu, update.callgraph.eu — see counts.py).

Accepts the daily banded usage counts that `callgraph.sh telemetry on` users send (POST JSON, see
src/stats/telemetry/payload.rs). Stores nothing that identifies a person or a machine: no IP address,
no headers, no file names — only the counts, per day and per batch id (a random id per day, used to drop
duplicates). Raw days are kept for 90 days, then folded into monthly totals.

Standard library only (http.server + sqlite3), so the container has no dependencies.
"""

import json
import re
import os
import sqlite3
import threading
import time
from datetime import date, timedelta
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import counts

DB_PATH = os.environ.get("TELEMETRY_DB", "/data/telemetry.sqlite")
PORT = int(os.environ.get("PORT", "8080"))
MAX_BODY = 64 * 1024
MAX_DAYS = 7
RETENTION_DAYS = 90
# the database never grows past this: new data is refused (the disk cannot be filled from outside)
MAX_DB_BYTES = int(os.environ.get("TELEMETRY_MAX_DB_BYTES", str(200 * 1024 * 1024)))
# no real day has more calls than this per command — larger numbers are fake, the batch is dropped
MAX_COUNT = 200_000
# per-client-address request budget (in memory only, never stored): a few sends per hour are normal
RATE_LIMIT = 30
RATE_WINDOW = 3600

_lock = threading.Lock()
_hits: dict[str, list[float]] = {}


def db() -> sqlite3.Connection:
    con = sqlite3.connect(DB_PATH, timeout=10)
    con.execute("PRAGMA journal_mode=WAL")
    con.executescript(
        """
        CREATE TABLE IF NOT EXISTS days (
            batch TEXT PRIMARY KEY,
            day TEXT NOT NULL,
            version TEXT NOT NULL,
            platform TEXT NOT NULL,
            commands TEXT NOT NULL,
            received TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS days_day ON days(day);
        CREATE TABLE IF NOT EXISTS hits (
            day TEXT NOT NULL,
            page TEXT NOT NULL,
            referrer TEXT NOT NULL,
            event TEXT NOT NULL,
            count INTEGER NOT NULL,
            PRIMARY KEY (day, page, referrer, event)
        );
        CREATE TABLE IF NOT EXISTS months (
            month TEXT NOT NULL,
            version TEXT NOT NULL,
            platform TEXT NOT NULL,
            batches INTEGER NOT NULL,
            commands TEXT NOT NULL,
            PRIMARY KEY (month, version, platform)
        );
        """
    )
    con.executescript(counts.SCHEMA)
    con.executescript(
        """
        CREATE TABLE IF NOT EXISTS edges (
            day TEXT NOT NULL, version TEXT NOT NULL, platform TEXT NOT NULL,
            src TEXT NOT NULL, dst TEXT NOT NULL, err INTEGER NOT NULL, rel TEXT NOT NULL, gap TEXT NOT NULL,
            count INTEGER NOT NULL,
            PRIMARY KEY (day, version, platform, src, dst, err, rel, gap)
        );
        CREATE TABLE IF NOT EXISTS session_stats (
            batch TEXT PRIMARY KEY, day TEXT NOT NULL, version TEXT NOT NULL, platform TEXT NOT NULL,
            count INTEGER NOT NULL, len TEXT NOT NULL, dur TEXT NOT NULL, rep_max TEXT NOT NULL
        );
        """
    )
    return con


def split_edge(key: str) -> tuple[str, str, int, str, str]:
    """`from>to|rel|gap` → (src, dst, err, rel, gap); az `other` gyűjtő-kulcs külön sor."""
    if key == "other":
        return "other", "other", 0, "-", "-"
    pair, rel, gap = key.split("|")
    src, dst = pair.split(">")
    err = 1 if src.endswith("!") else 0
    return src.rstrip("!"), dst, err, rel, gap


def _short(value, limit=40) -> bool:
    return isinstance(value, str) and 0 < len(value) <= limit


def _counts_only(obj, depth=0) -> bool:
    """The command data may only hold numbers, lists of numbers and nested objects with short keys."""
    if depth > 4:
        return False
    if isinstance(obj, bool):
        return False
    if isinstance(obj, (int, float)):
        return 0 <= obj <= MAX_COUNT
    if isinstance(obj, list):
        return len(obj) <= 16 and all(
            isinstance(v, (int, float)) and not isinstance(v, bool) and 0 <= v <= MAX_COUNT for v in obj
        )
    if isinstance(obj, dict):
        return len(obj) <= 64 and all(
            _short(k, 32) and all(c.isalnum() or c in "_-" for c in k) and _counts_only(v, depth + 1)
            for k, v in obj.items()
        )
    return False


# schema 2 (v0.9.5): „melyik parancs után melyik jött” élek — csak zárt nevek és sávok, név / út nem férhet bele
EDGE_RE = re.compile(
    r"^(start|[a-z_]{1,20}!?)>(end|[a-z_]{1,20})\|(same_file|same_dir|same_project|other_project|none)"
    r"\|(<5s|5-60s|1-10m|10-30m|-)$"
)
MAX_EDGES = 600
SESSION_BANDS = {"len": 5, "dur": 4, "rep_max": 3}


def _count(v) -> bool:
    return isinstance(v, int) and not isinstance(v, bool) and 0 <= v <= MAX_COUNT


def validate_edges(edges) -> bool:
    if edges is None:
        return True
    return (isinstance(edges, dict) and len(edges) <= MAX_EDGES
            and all((k == "other" or EDGE_RE.match(k)) and _count(v) for k, v in edges.items()))


def validate_sessions(sessions) -> bool:
    if sessions is None:
        return True
    if not isinstance(sessions, dict) or set(sessions) - {"count", *SESSION_BANDS}:
        return False
    if not _count(sessions.get("count", 0)):
        return False
    for key, n in SESSION_BANDS.items():
        band = sessions.get(key, [0] * n)
        if not isinstance(band, list) or len(band) != n or not all(_count(v) for v in band):
            return False
    return True


def validate(payload) -> list[dict] | None:
    if not isinstance(payload, dict) or payload.get("schema") not in (1, 2):
        return None
    schema = payload["schema"]
    if not _short(payload.get("version"), 32) or not _short(payload.get("platform"), 32):
        return None
    days = payload.get("days")
    if not isinstance(days, list) or not 0 < len(days) <= MAX_DAYS:
        return None
    today = date.today()
    out = []
    for d in days:
        allowed = {"day", "batch", "commands"} | ({"edges", "sessions"} if schema == 2 else set())
        if not isinstance(d, dict) or set(d) - allowed:
            return None
        try:
            day = date.fromisoformat(d.get("day", ""))
        except ValueError:
            return None
        # closed days only, not older than the catch-up window (+1 for time zones)
        if not today - timedelta(days=MAX_DAYS + 1) <= day < today + timedelta(days=1):
            return None
        batch = d.get("batch")
        if not _short(batch, 64) or not all(c in "0123456789abcdef-" for c in batch):
            return None
        if not _counts_only(d.get("commands")):
            return None
        if not validate_edges(d.get("edges")) or not validate_sessions(d.get("sessions")):
            return None
        out.append({"day": day.isoformat(), "batch": batch, "commands": d["commands"],
                    "edges": d.get("edges") or {}, "sessions": d.get("sessions")})
    return out


# website page views: a page path, the referring site's domain and the event — nothing else
PAGE_RE = re.compile(r"^/[a-z0-9/_.-]{0,80}$")
HOST_RE = re.compile(r"^[a-z0-9.-]{1,64}$")
EVENTS = ("view", "download")
SITE_ORIGIN = "https://callgraph.eu"
HIT_LIMIT = 600
# check / update requests per address and hour that are counted (a NAT can hide many installs);
# above it the request still works, it is only not counted
COUNT_LIMIT = 600


def validate_hit(obj) -> tuple[str, str, str] | None:
    if not isinstance(obj, dict) or set(obj) - {"p", "r", "e"}:
        return None
    page, ref, event = obj.get("p"), obj.get("r", ""), obj.get("e")
    if not isinstance(page, str) or not PAGE_RE.match(page) or ".." in page:
        return None
    if not isinstance(ref, str) or (ref and not HOST_RE.match(ref)) or event not in EVENTS:
        return None
    ref = ref[4:] if ref.startswith("www.") else ref
    return page, ref, event


def _db_full() -> bool:
    try:
        return sum(os.path.getsize(DB_PATH + ext) for ext in ("", "-wal") if os.path.exists(DB_PATH + ext)) > MAX_DB_BYTES
    except OSError:
        return False


def _rate_ok(client: str, limit: int = RATE_LIMIT) -> bool:
    now = time.time()
    with _lock:
        hits = [t for t in _hits.get(client, []) if now - t < RATE_WINDOW]
        if len(hits) >= limit:
            _hits[client] = hits
            return False
        hits.append(now)
        _hits[client] = hits
        if len(_hits) > 10000:
            _hits.clear()
        return True


def _merge(a, b):
    """Sums two count trees of the same shape (missing parts count as zero)."""
    if a is None:
        return b
    if b is None:
        return a
    if isinstance(a, dict) and isinstance(b, dict):
        return {k: _merge(a.get(k), b.get(k)) for k in set(a) | set(b)}
    if isinstance(a, list) and isinstance(b, list):
        n = max(len(a), len(b))
        return [(a[i] if i < len(a) else 0) + (b[i] if i < len(b) else 0) for i in range(n)]
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return a + b
    return b


def fold_old_days(con: sqlite3.Connection) -> None:
    """Raw days older than RETENTION_DAYS become monthly totals (per version and platform)."""
    cutoff = (date.today() - timedelta(days=RETENTION_DAYS)).isoformat()
    rows = con.execute("SELECT batch, day, version, platform, commands FROM days WHERE day < ?", (cutoff,)).fetchall()
    for batch, day, version, platform, commands in rows:
        month = day[:7]
        cur = con.execute(
            "SELECT batches, commands FROM months WHERE month=? AND version=? AND platform=?", (month, version, platform)
        ).fetchone()
        merged = _merge(json.loads(cur[1]), json.loads(commands)) if cur else json.loads(commands)
        con.execute(
            "INSERT INTO months(month, version, platform, batches, commands) VALUES (?,?,?,?,?) "
            "ON CONFLICT(month, version, platform) DO UPDATE SET batches=batches+1, commands=excluded.commands",
            (month, version, platform, 1, json.dumps(merged, separators=(",", ":"))),
        )
        con.execute("DELETE FROM days WHERE batch=?", (batch,))
    con.commit()


class Handler(BaseHTTPRequestHandler):
    server_version = "callgraph-telemetry"
    sys_version = ""

    def log_message(self, *args):  # no access log: no addresses are written anywhere
        pass

    def _reply(self, code: int, text: str = "") -> None:
        body = text.encode()
        self.send_response(code)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path, _, query = self.path.partition("?")
        if path.rstrip("/").endswith("/health"):
            return self._reply(200, "ok")
        if path == "/v1/check":
            return self._check(query)
        if path == "/v1/latest":
            return self._latest(query)
        if path.startswith("/v1/download/"):
            return self._download(path, query)
        return self._reply(404)

    def _redirect(self, location: str) -> None:
        self.send_response(302)
        self.send_header("Location", location)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def _counted(self, budget: str, limit: int, write) -> None:
        """Runs a counter update unless the address is over its budget or the database is full (never fails the request)."""
        if not _rate_ok(budget + ":" + (self.headers.get("X-Real-IP") or self.client_address[0]), limit) or _db_full():
            return
        with _lock:
            con = db()
            try:
                write(con)
                con.commit()
            except sqlite3.Error:
                pass
            finally:
                con.close()

    def _check(self, query: str):
        row = counts.parse_check(query)
        if row is not None:
            self._counted("check", COUNT_LIMIT, lambda con: counts.count_check(con, row))
        # 204 also for a malformed check: the client must not retry
        return self._reply(204)

    def _latest(self, query: str):
        tag = counts.latest_tag()
        if tag is None:
            return self._reply(503, "latest release unknown")
        version, platform = counts.parse_client(query)
        self._counted("update", COUNT_LIMIT, lambda con: counts.count_update(con, "check", version, platform, tag))
        return self._redirect(f"{counts.REPO}/releases/tag/{tag}")

    def _download(self, path: str, query: str):
        parsed = counts.parse_download(path, query)
        if parsed is None:
            return self._reply(404)
        tag, asset, kind = parsed
        if counts.counted_asset(asset):
            version, _ = counts.parse_client(query)
            platform = counts.asset_platform(asset)
            self._counted("update", COUNT_LIMIT, lambda con: counts.count_update(con, kind, version, platform, tag))
        return self._redirect(counts.download_url(tag, asset))

    def do_OPTIONS(self):
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", SITE_ORIGIN)
        self.send_header("Access-Control-Allow-Methods", "POST")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def _hit(self):
        client = "hit:" + (self.headers.get("X-Real-IP") or self.client_address[0])
        if not _rate_ok(client, HIT_LIMIT):
            return self._reply(429)
        try:
            length = int(self.headers.get("Content-Length") or 0)
            hit = validate_hit(json.loads(self.rfile.read(length))) if 0 < length <= 512 else None
        except (ValueError, UnicodeDecodeError):
            hit = None
        if hit is None:
            return self._reply(422)
        if _db_full():
            return self._reply(507)
        with _lock:
            con = db()
            try:
                con.execute(
                    "INSERT INTO hits(day, page, referrer, event, count) VALUES (?,?,?,?,1) "
                    "ON CONFLICT(day, page, referrer, event) DO UPDATE SET count=count+1",
                    (date.today().isoformat(), *hit),
                )
                con.commit()
            finally:
                con.close()
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", SITE_ORIGIN)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self):
        route = self.path.split("?")[0].rstrip("/")
        if route == "/hit":
            return self._hit()
        if route not in ("/v1", "/callgraph/telemetry"):
            return self._reply(404)
        client = self.headers.get("X-Real-IP") or self.client_address[0]
        if not _rate_ok(client):
            return self._reply(429, "too many requests")
        try:
            length = int(self.headers.get("Content-Length") or 0)
        except ValueError:
            return self._reply(400)
        if not 0 < length <= MAX_BODY:
            return self._reply(413 if length > MAX_BODY else 400)
        try:
            payload = json.loads(self.rfile.read(length))
        except (ValueError, UnicodeDecodeError):
            return self._reply(400, "bad json")
        days = validate(payload)
        if days is None:
            return self._reply(422, "rejected")
        if _db_full():
            return self._reply(507, "storage full")
        received = date.today().isoformat()
        with _lock:
            con = db()
            try:
                for d in days:
                    cur = con.execute(
                        "INSERT OR IGNORE INTO days(batch, day, version, platform, commands, received) VALUES (?,?,?,?,?,?)",
                        (d["batch"], d["day"], payload["version"], payload["platform"],
                         json.dumps(d["commands"], separators=(",", ":")), received),
                    )
                    if cur.rowcount != 1:
                        continue  # ugyanaz a köteg már megjött: az élek se duplázódjanak
                    for key, n in d["edges"].items():
                        con.execute(
                            "INSERT INTO edges(day, version, platform, src, dst, err, rel, gap, count) VALUES (?,?,?,?,?,?,?,?,?) "
                            "ON CONFLICT(day, version, platform, src, dst, err, rel, gap) DO UPDATE SET count=count+excluded.count",
                            (d["day"], payload["version"], payload["platform"], *split_edge(key), n),
                        )
                    s = d["sessions"]
                    if s:
                        con.execute(
                            "INSERT OR IGNORE INTO session_stats(batch, day, version, platform, count, len, dur, rep_max) "
                            "VALUES (?,?,?,?,?,?,?,?)",
                            (d["batch"], d["day"], payload["version"], payload["platform"], s.get("count", 0),
                             json.dumps(s.get("len", [0] * 5)), json.dumps(s.get("dur", [0] * 4)),
                             json.dumps(s.get("rep_max", [0] * 3))),
                        )
                con.commit()
                fold_old_days(con)
            finally:
                con.close()
        return self._reply(204)


def main() -> None:
    db().close()
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
