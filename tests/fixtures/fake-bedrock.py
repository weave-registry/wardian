# A fake Amazon Bedrock runtime for tests (ADR-2610071106). It checks a request the way Bedrock
# does — the URL, the body and the sign-in, with the SigV4 signature recomputed from the known test
# secret — and answers like Bedrock on errors. A good request is handed on to a fake Anthropic API
# (fake-anthropic.py or fake-builder.py) as a Messages request, so both providers get the same
# answers. GET /prompts and /log are passed through, for the tests that read them. GET /seen says
# how each request signed in; GET /signed lists the access key ID and session token of each SigV4
# request, for the AWS profile runs (ADR-2610091530).
#
# usage: fake-bedrock.py PORT UPSTREAM_URL
import hashlib, hmac, json, re, sys, urllib.parse, urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer

TOKEN = "test-bedrock-token"
# Access key ID → secret. ASIAPROFILETEST is what the profile runs' credential_process and fake
# `aws` print, with a session token that must be signed too.
SECRETS = {"AKIDTEST": "test-secret", "ASIAPROFILETEST": "profile-test-secret"}
PROFILE_SESSION = "profile-test-session-token"
MODELS = {"us.anthropic.claude-sonnet-4-5-20250929-v1:0", "us.anthropic.claude-haiku-4-5-20251001-v1:0"}
UPSTREAM = sys.argv[2].rstrip("/")
SEEN = []   # how each request signed in: "bearer" or "sigv4"
SIGNED = [] # {key_id, session_token} of each good SigV4 request

def uri_encode(s, keep_slash):
    safe = "-_.~" + ("/" if keep_slash else "")
    return "".join(c if (c.isascii() and c.isalnum()) or c in safe else "".join("%%%02X" % b for b in c.encode()) for c in s)

def sign_key(secret, date, region):
    k = ("AWS4" + secret).encode()
    for part in (date, region, "bedrock", "aws4_request"):
        k = hmac.new(k, part.encode(), hashlib.sha256).digest()
    return k

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass

    def reply(self, code, obj, kind=None):
        b = json.dumps(obj).encode(); self.send_response(code)
        self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(b)))
        if kind: self.send_header("x-amzn-ErrorType", kind + ":http://internal.amazon.com/coral/com.amazon.bedrock/")
        self.end_headers(); self.wfile.write(b)

    def do_GET(self):
        if self.path == "/seen": return self.reply(200, SEEN)
        if self.path == "/signed": return self.reply(200, SIGNED)
        with urllib.request.urlopen(UPSTREAM + self.path) as r:
            return self.reply(r.status, json.loads(r.read()))

    def signed_in(self, payload):
        auth = self.headers.get("Authorization", "")
        if auth == "Bearer " + TOKEN:
            SEEN.append("bearer"); return None
        m = re.match(r"AWS4-HMAC-SHA256 Credential=([^/]+)/(\d{8})/([a-z0-9-]+)/bedrock/aws4_request, SignedHeaders=([a-z0-9;-]+), Signature=([0-9a-f]{64})$", auth)
        if not m:
            return "The security token included in the request is invalid."
        key_id, date, region, signed, sig = m.groups()
        if key_id not in SECRETS:
            return "The security token included in the request is invalid."
        session = self.headers.get("X-Amz-Security-Token", "")
        if key_id.startswith("ASIA") and (session != PROFILE_SESSION or "x-amz-security-token" not in signed.split(";")):
            return "The security token included in the request is invalid."
        names = signed.split(";")
        if "host" not in names or "x-amz-date" not in names:
            return "SignedHeaders must include host and x-amz-date."
        amz_date = self.headers.get("X-Amz-Date", "")
        canonical_headers = "".join("%s:%s\n" % (n, (self.headers.get(n) or "").strip()) for n in names)
        canonical = "\n".join(["POST", uri_encode(self.path, True), "", canonical_headers, signed, hashlib.sha256(payload).hexdigest()])
        scope = "%s/%s/bedrock/aws4_request" % (date, region)
        to_sign = "\n".join(["AWS4-HMAC-SHA256", amz_date, scope, hashlib.sha256(canonical.encode()).hexdigest()])
        want = hmac.new(sign_key(SECRETS[key_id], date, region), to_sign.encode(), hashlib.sha256).hexdigest()
        if not hmac.compare_digest(want, sig):
            return "The request signature we calculated does not match the signature you provided."
        SEEN.append("sigv4"); SIGNED.append({"key_id": key_id, "session_token": session}); return None

    def do_POST(self):
        payload = self.rfile.read(int(self.headers["Content-Length"]))
        m = re.match(r"^/model/([^/]+)/invoke$", self.path)
        if not m: return self.reply(404, {"message": "Not found"}, "UnknownOperationException")
        why = self.signed_in(payload)
        if why: return self.reply(403, {"message": why}, "UnrecognizedClientException")
        model = urllib.parse.unquote(m.group(1))
        if model not in MODELS:
            return self.reply(403, {"message": "You don't have access to the model with the specified model ID."}, "AccessDeniedException")
        body = json.loads(payload)
        if body.get("anthropic_version") != "bedrock-2023-05-31" or "model" in body:
            return self.reply(400, {"message": "Malformed input request: anthropic_version missing or model in body"}, "ValidationException")
        # Wardian tests a key with one 1-token request; answer it here, so the fake Claude behind
        # counts only the real work.
        if body.get("max_tokens") == 1:
            return self.reply(200, {"content": [{"type": "text", "text": "H"}], "stop_reason": "max_tokens", "role": "assistant"})
        body.pop("anthropic_version")
        body["model"] = model
        req = urllib.request.Request(UPSTREAM + "/v1/messages", data=json.dumps(body).encode(), method="POST",
                                     headers={"Content-Type": "application/json", "x-api-key": "test-key"})
        try:
            with urllib.request.urlopen(req) as r:
                return self.reply(r.status, json.loads(r.read()))
        except urllib.error.HTTPError as e:
            return self.reply(e.code, {"message": e.read().decode()[:200]}, "ValidationException")

HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
