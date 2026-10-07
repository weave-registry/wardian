# A fake Anthropic API for tests: answers the USL lab's prompts with canned replies,
# and keeps every prompt so the test can check what was sent (GET /prompts).
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
PROMPTS = []
# "org-key" is not scoped to a workspace: like the real API, it needs anthropic-workspace-id.
WS_ERROR = "This API key is not scoped to a workspace, so this request must include the anthropic-workspace-id header with the ID of the workspace to use."
class H(BaseHTTPRequestHandler):
    def refused(self):
        k = self.headers.get("x-api-key")
        if k == "org-key":
            if self.headers.get("anthropic-workspace-id") == "wrkspc_test": return False
            self.reply(400, {"type": "error", "error": {"type": "invalid_request_error", "message": WS_ERROR}}); return True
        if k != "test-key":
            self.reply(401, {"error": {"message": "invalid x-api-key"}}); return True
        return False
    def reply(self, code, obj):
        b = json.dumps(obj).encode(); self.send_response(code)
        self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(b))); self.end_headers(); self.wfile.write(b)
    def log_message(self, *a): pass
    def do_GET(self):
        if self.path == "/prompts": return self.reply(200, PROMPTS)
        if self.refused(): return
        self.reply(200, {"id": "model"})
    def do_POST(self):
        if self.refused(): return
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        prompt = body["messages"][0]["content"]
        PROMPTS.append({"model": body["model"], "prompt": prompt})
        if "Pick the one index" in prompt:
            text = '{"index":"loadtest","sourcetype":"jmeter","why":"It holds the load-test steps."}'
        elif "Write a Splunk search" in prompt:
            text = json.dumps({"search": 'index="loadtest" sourcetype="jmeter" | stats avg(throughput) AS x BY concurrency | table concurrency x r',
                               "loadUnit": "users", "throughputUnit": "req/s", "responseUnit": "ms", "earliest": "-7d",
                               "explanation": "Average throughput at each concurrency step of the load test.",
                               "title": "JMeter load test steps", "load": "Virtual users running in each step.",
                               "throughput": "Requests finished per second in that step.", "response": "Average response time, in milliseconds.",
                               "method": "One point per concurrency step, averaged over the step."})
        elif "Convert it into multiplicative factors" in prompt:
            text = '{"alphaFactor":0.5,"betaFactor":1,"lambdaFactor":1,"rationale":"Halving lock time halves contention."}'
        else:
            text = "## What the curve says\nThroughput peaks near 48 users.\n## Likely causes\n- a hypothesis"
        self.reply(200, {"content": [{"type": "text", "text": text}], "stop_reason": "end_turn"})
HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
