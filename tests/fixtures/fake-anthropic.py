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
        # The apps' AI runs a tool loop (the agent component, ADR-2610131300): each request carries the
        # skill, the task and the steps so far, and the reply is one JSON object, a tool or the answer.
        steps = prompt.split("<steps>", 1)[1].split("</steps>", 1)[0] if "<steps>" in prompt else ""
        if "Write a Splunk search (SPL)" in prompt and "3. sample_events" not in steps:
            if "2. field_summary" in steps: text = json.dumps({"tool": "sample_events", "input": {"index": "loadtest", "sourcetype": "jmeter"}})
            elif "1. list_data" in steps: text = json.dumps({"tool": "field_summary", "input": {"index": "loadtest", "sourcetype": "jmeter"}})
            else: text = json.dumps({"tool": "list_data", "input": {}})
        elif "Write a Splunk search (SPL)" in prompt:
            text = json.dumps({"answer": {"search": 'index="loadtest" sourcetype="jmeter" | stats avg(throughput) AS x BY concurrency | table concurrency x r',
                               "loadUnit": "users", "throughputUnit": "req/s", "responseUnit": "ms", "earliest": "-7d",
                               "explanation": "Average throughput at each concurrency step of the load test.",
                               "title": "JMeter load test steps", "load": "Virtual users running in each step.",
                               "throughput": "Requests finished per second in that step.", "response": "Average response time, in milliseconds.",
                               "method": "One point per concurrency step, averaged over the step."}})
        elif "Turn this proposed change into factors" in prompt:
            text = '{"answer": {"alphaFactor":0.5,"betaFactor":1,"lambdaFactor":1,"rationale":"Halving lock time halves contention."}}'
        elif "Write the diagnosis of this USL fit" in prompt:
            text = json.dumps({"answer": "## What the curve says\nThroughput peaks near 48 users.\n## Likely causes\n- a hypothesis"})
        else:
            text = "## What the curve says\nThroughput peaks near 48 users.\n## Likely causes\n- a hypothesis"
        # Usage as the real API reports it, so Wardian can count it (ADR-2610081500).
        self.reply(200, {"content": [{"type": "text", "text": text}], "stop_reason": "end_turn", "usage": {"input_tokens": 120, "output_tokens": 40}})
HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
