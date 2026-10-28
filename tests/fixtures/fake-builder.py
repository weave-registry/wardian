# A fake Anthropic API that builds an app through Wardian's tools, slowly, like Claude would, in the
# Elephant-Goldfish Model (ADR-2610281200): it writes the plan, asks for a design review, stops for
# the person's approval, builds after "go", asks for a build review, then saves. It also answers the
# goldfish reviewers' requests. The first version of the app fails in the browser; after a
# "[browser test]" message it writes a fixed one. GET /log lists what each reply did, so the test
# can check the order of events.
import json, sys, time
from http.server import BaseHTTPRequestHandler, HTTPServer
LOG = []
SUITE = json.dumps({"format": 1, "title": "Background test", "description": "Built by the fake Claude.",
                    "apps": [{"name": "main", "slot": "main"}]})
VIEW = "<h2>Hello from the background</h2><p id=\"out\">ready</p>"
README = "# Background test\nSays hello.\n\n## Layout\nOne panel with the greeting."
BROKEN = "Kernel.register({ name: 'main', init(ctx) { throw new Error('boom: the first version is broken'); } });"
FIXED = "Kernel.register({ name: 'main', init(ctx) { ctx.$('#out').textContent = 'fixed'; } });"

def tool_use(i, name, inp):
    return {"type": "tool_use", "id": "t%d_%s" % (i, name), "name": name, "input": inp}

class H(BaseHTTPRequestHandler):
    def reply(self, code, obj):
        b = json.dumps(obj).encode(); self.send_response(code)
        self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(b))); self.end_headers(); self.wfile.write(b)
    def log_message(self, *a): pass
    def do_GET(self):
        if self.path == "/log": return self.reply(200, LOG)
        self.reply(200, {"id": "model"})
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        system = body.get("system") or []
        if isinstance(system, list) and system and str(system[0].get("text", "")).startswith("You are a goldfish reviewer"):
            LOG.append("goldfish")   # a reviewer: its own request, with none of the chat
            return self.reply(200, {"content": [{"type": "text", "text": "## Findings\nNone.\n## Decisions for the user\nNone."}], "stop_reason": "end_turn"})
        msgs = body["messages"]
        last = msgs[-1]
        texts = [b.get("text", "") for b in last["content"] if isinstance(b, dict) and b.get("type") == "text"] if isinstance(last["content"], list) else [last["content"]]
        results = [b for b in last["content"] if isinstance(b, dict) and b.get("type") == "tool_result"] if isinstance(last["content"], list) else []
        prev = next((m for m in reversed(msgs[:-1]) if m["role"] == "assistant"), None)
        calls = [b for b in (prev or {}).get("content", []) if b.get("type") == "tool_use"]
        prev_tools = [b["name"] for b in calls]
        written = [b["input"].get("path") for b in calls if b["name"] == "write_file"]
        n = len(msgs)
        time.sleep(1.2)  # long enough for the test to leave the chat while Claude works
        if any("[browser test]" in t for t in texts):
            LOG.append("fix")
            content = [{"type": "text", "text": "The browser test found a problem; fixing it."},
                       tool_use(n, "write_file", {"path": "apps/main/app.js", "content": FIXED})]
        elif any(t.strip().lower() == "go" for t in texts) and not results:
            LOG.append("write")
            content = [{"type": "text", "text": "Writing the app."},
                       tool_use(n, "write_file", {"path": "apps/main/view.html", "content": VIEW}),
                       tool_use(n + 1, "write_file", {"path": "apps/main/app.js", "content": BROKEN})]
        elif texts and not results:
            LOG.append("plan")
            content = [{"type": "text", "text": "Writing the plan."},
                       tool_use(n, "write_file", {"path": "README.md", "content": README}),
                       tool_use(n + 1, "write_file", {"path": "suite.json", "content": SUITE})]
        elif "README.md" in written:
            LOG.append("design-review")
            content = [tool_use(n, "fresh_review", {"stage": "design", "asked": "a small app that says hello"})]
        elif "fresh_review" in prev_tools and calls[0]["input"].get("stage") == "design":
            LOG.append("approve")
            return self.reply(200, {"content": [{"type": "text", "text": "Here is the plan: one panel that says hello. Reply go, or say what to change."}], "stop_reason": "end_turn"})
        elif "apps/main/view.html" in written:
            LOG.append("components")
            content = [tool_use(n, "add_components", {"components": ["button", "card"]})]
        elif "add_components" in prev_tools:
            LOG.append("build-review")
            content = [tool_use(n, "fresh_review", {"stage": "build"})]
        elif "fresh_review" in prev_tools or written == ["apps/main/app.js"]:
            LOG.append("finish")
            content = [tool_use(n, "finish", {"name": "bg-test", "summary": "A small test app."})]
        else:
            LOG.append("done")
            return self.reply(200, {"content": [{"type": "text", "text": "Saved. Open it from the list."}], "stop_reason": "end_turn"})
        self.reply(200, {"content": content, "stop_reason": "tool_use"})
HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
