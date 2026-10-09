# A fake Splunk management port for tests: server info and oneshot searches.
# Token "test-token". A search containing "badsyntax" fails like Splunk does.
# A search containing "slowtable" stays running for SLOW seconds (ADR-2610072118), and a job can be
# cancelled (POST .../control action=cancel); GET /fake/cancelled lists the cancelled job ids.
#
# Usage: server.py PORT                 the management API over plain HTTP
#        server.py PORT CERT KEY        the management API over TLS, with this certificate
#        server.py PORT --web404        Splunk Web: "Page not found!" pages for /services/...
#        server.py PORT --web303        Splunk Web: every page sends the browser to the login page
# (ADR-2610091500: setup finds the API behind the web address.)
import json, ssl, sys, time, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
TOKEN = "Bearer test-token"
SLOW = 12
JOBS = {}
READY_AT = {}    # sid -> when a slow job is done
CANCELLED = []
class H(BaseHTTPRequestHandler):
    def reply(self, code, obj):
        b = json.dumps(obj).encode(); self.send_response(code)
        self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(b))); self.end_headers(); self.wfile.write(b)
    def authed(self):
        if self.headers.get("Authorization") != TOKEN:
            self.reply(401, {"messages": [{"type": "WARN", "text": "call not properly authenticated"}]}); return False
        return True
    def do_GET(self):
        if self.path == "/fake/cancelled":
            return self.reply(200, CANCELLED)
        # Like real Splunk: server/info answers anyone; everything else needs the token.
        if self.path.startswith("/services/search/jobs/"):
            if not self.authed(): return
            sid = self.path.split("/")[4].split("?")[0]
            job = JOBS.get(sid)
            if not job: return self.reply(404, {"messages": [{"type": "ERROR", "text": "Unknown sid."}]})
            if "/results" in self.path:
                # Like Splunk: count and offset page through the results (count 0 means all).
                q = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
                if job[0] != 200 or "results" not in job[1]: return self.reply(job[0], job[1])
                offset = int(q.get("offset", ["0"])[0]); count = int(q.get("count", ["100"])[0])
                rows = job[1]["results"]
                page = rows[offset:] if count == 0 else rows[offset:offset + count]
                return self.reply(200, dict(job[1], results=page))
            if sid in CANCELLED:
                return self.reply(200, {"entry": [{"content": {"isDone": False, "isFailed": False, "dispatchState": "FINALIZING"}}]})
            if time.time() < READY_AT.get(sid, 0):
                return self.reply(200, {"entry": [{"content": {"isDone": False, "isFailed": False, "dispatchState": "RUNNING"}}]})
            failed = job[0] != 200
            return self.reply(200, {"entry": [{"content": {"isDone": True, "isFailed": failed, "dispatchState": "FAILED" if failed else "DONE",
                                                            "messages": job[1].get("messages", []) if failed else []}}]})
        if self.path.startswith("/services/server/info"):
            return self.reply(200, {"entry": [{"content": {"serverName": "fake-splunk", "version": "9.3.0"}}]})
        if not self.authed(): return
        if self.path.startswith("/services/authentication/current-context"):
            return self.reply(200, {"entry": [{"content": {"username": "wardian-reader"}}]})
        self.reply(404, {"messages": [{"type": "ERROR", "text": "Not Found"}]})
    def do_POST(self):
        if not self.authed(): return
        form = urllib.parse.parse_qs(self.rfile.read(int(self.headers["Content-Length"])).decode())
        if self.path.rstrip("/") == "/services/search/jobs":
            # Start a job: work out its answer now, hand back a sid, serve the answer from /results.
            code, body = self.answer(form)
            sid = "job%d" % (len(JOBS) + 1)
            JOBS[sid] = (code, body)
            if "slowtable" in form["search"][0]:
                READY_AT[sid] = time.time() + SLOW
            return self.reply(201, {"sid": sid})
        if self.path.startswith("/services/search/jobs/") and self.path.endswith("/control"):
            sid = self.path.split("/")[4]
            if form.get("action") == ["cancel"] and sid in JOBS:
                CANCELLED.append(sid)
                sys.stderr.write("CANCEL %s\n" % sid)
                return self.reply(200, {"messages": [{"type": "INFO", "text": "Search job cancelled."}]})
        self.reply(404, {})

    def reply_to(self, code, obj):
        return (code, obj)

    def answer(self, form):
        self.reply = self.reply_to  # the branches below "reply"; capture instead of sending
        try:
            return self.search(form)
        finally:
            del self.reply

    def search(self, form):
        sys.stderr.write("SEARCH %r earliest=%r\n" % (form.get("search"), form.get("earliest_time")))
        if "badsyntax" in form["search"][0]:
            return self.reply(400, {"messages": [{"type": "FATAL", "text": "Unknown search command 'badsyntax'."}]})
        q = form["search"][0]
        if "tstats" in q:
            return self.reply(200, {"fields": [{"name": "index"}, {"name": "sourcetype"}, {"name": "count"}],
                                    "results": [{"index": "loadtest", "sourcetype": "jmeter", "count": "5000"}, {"index": "main", "sourcetype": "syslog", "count": "90"}]})
        if "fieldsummary" in q:
            return self.reply(200, {"fields": [{"name": "field"}, {"name": "count"}, {"name": "distinct_count"}],
                                    "results": [{"field": "concurrency", "count": "5000", "distinct_count": "8"}, {"field": "throughput", "count": "5000", "distinct_count": "4000"}]})
        if "_raw" in q:
            return self.reply(200, {"fields": [{"name": "_raw"}], "results": [{"_raw": "step=3 concurrency=4 throughput=3400 user=ann.lee@example.com"}]})
        rows = []
        if "bigtable" in q:  # 50,000 rows, for loading a large table into the database
            for i in range(50000):
                rows.append({"n": str(i + 1), "host": "web-%d" % (i % 7), "status": "500" if i % 10 == 0 else "200", "ms": "%.1f" % (10 + (i * 37) % 900)})
            return self.reply(200, {"fields": [{"name": "n"}, {"name": "host"}, {"name": "status"}, {"name": "ms"}], "results": rows, "messages": []})
        if "slowtable" in q:  # 300 rows, after SLOW seconds
            for i in range(300):
                rows.append({"n": str(i + 1), "host": "web-%d" % (i % 3)})
            return self.reply(200, {"fields": [{"name": "n"}, {"name": "host"}], "results": rows, "messages": []})
        if "minutes" in q:  # a ready-made Little's Law search: n x r minutes
            for i, n in enumerate([0.5, 1, 1.5, 2, 3, 4]):
                x = 3 * n / (1 + 0.1 * (n - 1))
                rows.append({"n": str(n), "x": "%.3f" % x, "r": "%.0f" % (n / x * 1000), "minutes": str(400 - 60 * i)})
            return self.reply(200, {"fields": [{"name": "n"}, {"name": "x"}, {"name": "r"}, {"name": "minutes"}], "results": rows, "messages": []})
        for n in [1, 2, 4, 8, 16, 32, 48, 64]:
            x = 1000 * n / (1 + 0.05 * (n - 1) + 0.0004 * n * (n - 1))
            rows.append({"concurrency": str(n), "x": "%.1f" % x, "r": "%.2f" % (n / x * 1000), "_time": "2026-10-06"})
        return self.reply(200, {"fields": [{"name": "concurrency"}, {"name": "x"}, {"name": "r"}, {"name": "_time"}], "results": rows, "messages": []})

class Web(BaseHTTPRequestHandler):
    """Splunk Web, as it answers a client that asks it for the REST API."""
    redirect = False
    def do_GET(self):
        sys.stderr.write("WEB %s\n" % self.path)
        if self.redirect:
            self.send_response(303)
            self.send_header("Location", "/en-US/account/login?return_to=%s" % urllib.parse.quote(self.path))
            self.send_header("Content-Length", "0"); self.end_headers(); return
        b = b"<!doctype html><html><head><title>Page not found! - Splunk</title></head><body><h1>Oops.</h1><p>Page not found!</p></body></html>"
        self.send_response(404); self.send_header("Server", "Splunkd"); self.send_header("Content-Type", "text/html; charset=UTF-8")
        self.send_header("Content-Length", str(len(b))); self.end_headers(); self.wfile.write(b)

port = int(sys.argv[1]) if len(sys.argv) > 1 else 18089
mode = sys.argv[2:]
if mode in (["--web404"], ["--web303"]):
    Web.redirect = mode == ["--web303"]
    ThreadingHTTPServer(("127.0.0.1", port), Web).serve_forever()
server = ThreadingHTTPServer(("127.0.0.1", port), H)
if len(mode) == 2:
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.load_cert_chain(mode[0], mode[1])
    # The handshake runs in each request's own thread, so a client that never finishes it
    # (such as one speaking plain HTTP) holds up no one else.
    server.socket = tls.wrap_socket(server.socket, server_side=True, do_handshake_on_connect=False)
    H.setup = lambda self: (self.request.do_handshake(), BaseHTTPRequestHandler.setup(self))
server.serve_forever()
