#!/usr/bin/env python3
"""A fake Anthropic API for the App settings and Ask test (ADR-2610131300).
- Improve with AI (a system prompt that starts "You improve"): the skill and tools, with find_host added.
- The agent loop of the ai app (a prompt with <steps>): top_hosts first, then the answer from its result.
- The key test (GET /v1/models/<model>): the model. Anything else: a short text. Every request body is appended to the log file."""
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

PORT, LOG = int(sys.argv[1]), sys.argv[2]

def text_reply(text):
    return {"id": "msg_1", "type": "message", "role": "assistant", "model": "fake", "stop_reason": "end_turn",
            "content": [{"type": "text", "text": text}], "usage": {"input_tokens": 10, "output_tokens": 10}}

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        data = json.dumps({"id": self.path.rsplit("/", 1)[-1], "type": "model"}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        with open(LOG, "a") as f: f.write(json.dumps(body) + "\n")
        system = body.get("system") or ""
        system = system if isinstance(system, str) else json.dumps(system)
        msgs = body.get("messages") or [{}]
        prompt = msgs[0].get("content")
        prompt = prompt if isinstance(prompt, str) else json.dumps(prompt)
        if system.startswith("You improve"):
            out = {"skill": "Look the hosts up with top_hosts first. Name each host with its error count.",
                   "notes": "Named the method; added find_host for one host.",
                   "tools": [{"name": "top_hosts", "description": "The 1 to 10 hosts with the most errors, most first.",
                              "input": {"type": "object", "properties": {"limit": {"type": "integer", "minimum": 1, "maximum": 10}}, "required": ["limit"]}},
                             {"name": "find_host", "description": "One host's errors.",
                              "input": {"type": "object", "properties": {"host": {"type": "string", "maxLength": 100}}, "required": ["host"]}}]}
            reply = text_reply(json.dumps(out))
        elif "<steps>" in prompt:
            done = "1. top_hosts" in prompt
            reply = text_reply(json.dumps({"answer": {"reply": "web-1 had the most errors: 40."}} if done else {"tool": "top_hosts", "input": {"limit": 2}}))
        else:
            reply = text_reply("ok")
        data = json.dumps(reply).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

HTTPServer(("127.0.0.1", PORT), H).serve_forever()
