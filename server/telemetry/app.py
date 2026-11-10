"""callgraph opt-in telemetry receiver.

Accepts the daily banded usage counts that `callgraph.sh telemetry on` users send (POST JSON, see
src/stats/telemetry/payload.rs). Stores nothing that identifies a person or a machine: no IP address,
no headers, no file names — only the counts, per day and per batch id (a random id per day, used to drop
duplicates). Raw days are kept for 90 days, then folded into monthly totals.

Standard library only (http.server + sqlite3), so the container has no dependencies.
"""

import json
import os
import sqlite3
import threading
import time
from datetime import date, timedelta
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DB_PATH = os.environ.get("TELEMETRY_DB", "/data/telemetry.sqlite")
PORT = int(os.environ.get("PORT", "8080"))
MAX_BODY = 64 * 1024
MAX_DAYS = 7
RETENTION_DAYS = 90
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
    return con


def _short(value, limit=40) -> bool:
    return isinstance(value, str) and 0 < len(value) <= limit


def _counts_only(obj, depth=0) -> bool:
    """The command data may only hold numbers, lists of numbers and nested objects with short keys."""
    if depth > 4:
        return False
    if isinstance(obj, bool):
        return False
    if isinstance(obj, (int, float)):
        return obj >= 0
    if isinstance(obj, list):
        return len(obj) <= 16 and all(isinstance(v, (int, float)) and not isinstance(v, bool) and v >= 0 for v in obj)
    if isinstance(obj, dict):
        return len(obj) <= 64 and all(
            _short(k, 32) and all(c.isalnum() or c in "_-" for c in k) and _counts_only(v, depth + 1)
            for k, v in obj.items()
        )
    return False


def validate(payload) -> list[dict] | None:
    if not isinstance(payload, dict) or payload.get("schema") != 1:
        return None
    if not _short(payload.get("version"), 32) or not _short(payload.get("platform"), 32):
        return None
    days = payload.get("days")
    if not isinstance(days, list) or not 0 < len(days) <= MAX_DAYS:
        return None
    today = date.today()
    out = []
    for d in days:
        if not isinstance(d, dict) or set(d) - {"day", "batch", "commands"}:
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
        out.append({"day": day.isoformat(), "batch": batch, "commands": d["commands"]})
    return out


def _rate_ok(client: str) -> bool:
    now = time.time()
    with _lock:
        hits = [t for t in _hits.get(client, []) if now - t < RATE_WINDOW]
        if len(hits) >= RATE_LIMIT:
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
        if self.path.rstrip("/").endswith("/health"):
            return self._reply(200, "ok")
        return self._reply(404)

    def do_POST(self):
        if not self.path.rstrip("/").endswith("/callgraph/telemetry"):
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
        received = date.today().isoformat()
        with _lock:
            con = db()
            try:
                for d in days:
                    con.execute(
                        "INSERT OR IGNORE INTO days(batch, day, version, platform, commands, received) VALUES (?,?,?,?,?,?)",
                        (d["batch"], d["day"], payload["version"], payload["platform"],
                         json.dumps(d["commands"], separators=(",", ":")), received),
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
