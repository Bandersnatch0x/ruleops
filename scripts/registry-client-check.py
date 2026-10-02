#!/usr/bin/env python3
"""Real Java Server -> rf registry acceptance; stdlib only, no external endpoints/credentials."""
import copy
import hashlib
import http.client
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(sys.argv[1]).resolve()  # Require a fresh evidence directory from the caller.
OUT.mkdir(parents=True, exist_ok=False)
checks = []


def run(label, argv, expected=0, timeout=60):
    result = subprocess.run(argv, cwd=ROOT, text=True, capture_output=True, timeout=timeout)
    (OUT / (label + ".stdout")).write_text(result.stdout)
    (OUT / (label + ".stderr")).write_text(result.stderr)
    with (OUT / "rc.txt").open("a") as log:
        log.write(f"{label} rc={result.returncode} expected={expected}\n")
    assert result.returncode == expected, f"{label}: rc={result.returncode}, see {OUT}"
    return result


def cli(label, args, expected=0):
    result = run(label, [str(ROOT / "rf"), "registry", *args], expected, 15)
    value = json.loads(result.stdout)
    assert value["authority"] == "UNKNOWN"
    assert value["status"] == ("UNKNOWN" if expected else "RECEIVED_ONLY")
    if expected:
        assert "registry" not in value, label
        assert "not-for-output" not in result.stdout + result.stderr, label
    checks.append(label)
    return value


def request(endpoint, method, suffix, body=None, headers=None):
    url = urlsplit(endpoint)
    connection = http.client.HTTPConnection(url.hostname, url.port, timeout=3)
    try:
        connection.request(method, url.path.rstrip("/") + suffix, body=body, headers=headers or {})
        response = connection.getresponse()
        return response.status, json.loads(response.read())
    finally:
        connection.close()


def seed(endpoint, kind, identity, version):
    # Independent reproduction of the documented Store envelope: outer field order
    # is type/id/version/payload, and payload object keys are recursively sorted.
    payload = {"title": "registry fixture", "details": {"z": 2, "a": 1}}
    canonical_payload = json.loads(json.dumps(payload, sort_keys=True))
    envelope = {"type": kind, "id": identity, "version": version, "payload": canonical_payload}
    digest = "sha256:" + hashlib.sha256(json.dumps(envelope, separators=(",", ":")).encode()).hexdigest()
    status, value = request(endpoint, "POST", "/" + kind,
                         json.dumps({"id": identity, "version": version, "payload": payload}),
                         {"Content-Type": "application/json", "Idempotency-Key": f"{kind}-{identity}-{version}",
                          "X-Actor": "registry-check", "X-Reason": "local acceptance fixture"})
    assert status == 201, (status, value)
    assert value["resource"]["digest"] == digest
    return digest


def fault(label, status, body, args, delay=0):
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            if delay:
                time.sleep(delay)
                return
            data = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *_):
            pass  # Fault fixture logs contain no useful evidence; only CLI receipts are recorded.

    with http.server.HTTPServer(("127.0.0.1", 0), Handler) as server:
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.01})
        thread.start()
        try:
            return cli(label, [*args, "--server", f"http://127.0.0.1:{server.server_port}/v1"], 3)
        finally:
            server.shutdown()
            thread.join(timeout=2)
            assert not thread.is_alive()


run("cargo-build", ["cargo", "build", "-q", "-p", "erf-thin-client"])
classpath = OUT / "classpath.txt"
run("server-build", ["mvn", "-q", "-pl", "control-plane/server", "-am", "package", "-DskipTests",
                     "dependency:build-classpath", f"-Dmdep.outputFile={classpath}"])
with (OUT / "server.log").open("w") as log:
    server = subprocess.Popen(["java", "-cp", str(ROOT / "control-plane/server/target/classes") + os.pathsep + classpath.read_text().strip(),
                               "io.ruleops.server.RuleOpsServerMain", "0"], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    try:
        deadline = time.monotonic() + 10
        endpoint = None
        while time.monotonic() < deadline:
            assert server.poll() is None, "Java Server exited; inspect server.log"
            for line in (OUT / "server.log").read_text().splitlines():
                if line.startswith("ruleops-server="):
                    endpoint = line.split("=", 1)[1]
            if endpoint:
                break
            time.sleep(0.05)
        assert endpoint, "Java Server startup timeout"
        assert urlsplit(endpoint).hostname in ("127.0.0.1", "::1")
        empty = cli("empty-registry", ["list", "rules", "--server", endpoint])
        assert empty["registry"]["items"] == [] and empty["digestVerification"] == "FORMAT_ONLY"
        pins = {}
        for kind in ("rules", "standards"):
            pins[kind] = seed(endpoint, kind, "fixture", "1")
            seed(endpoint, kind, "fixture", "2")
        _, audit_before = request(endpoint, "GET", "/audit")
        for kind in ("rules", "standards"):
            status, raw = request(endpoint, "GET", f"/registry/{kind}?limit=1", headers={"X-Request-Id": "registry-read"})
            (OUT / (kind + "-raw.json")).write_text(json.dumps(raw, indent=2))
            assert status == 200
            page = cli(f"{kind}-list", ["list", kind, "--server", endpoint, "--limit", "1"])
            assert page["digestVerification"] == "FORMAT_ONLY"
            assert page["registry"]["hasMore"] and len(page["registry"]["items"]) == 1
            assert page["registry"]["items"][0]["digest"] == pins[kind]
            second = cli(f"{kind}-next", ["list", kind, "--server", endpoint, "--limit", "1", "--cursor-json", json.dumps(page["registry"]["nextCursor"])])
            assert not second["registry"]["hasMore"] and second["registry"]["items"][0]["version"] == "2"
            shown = cli(f"{kind}-show", ["show", kind, "fixture", "--version", "1", "--expected-digest", pins[kind], "--server", endpoint])
            assert shown["digestVerification"] == "PIN_MATCH" and shown["registry"]["resource"]["digest"] == pins[kind]
        show = ["show", "rules", "fixture", "--version", "1", "--expected-digest", pins["rules"]]
        assert cli("digest-mismatch", ["show", "rules", "fixture", "--version", "1", "--expected-digest", "sha256:" + "0" * 64, "--server", endpoint], 3)["code"] == "REGISTRY_DIGEST_MISMATCH"
        cli("missing-version", ["show", "rules", "fixture", "--expected-digest", pins["rules"], "--server", endpoint], 3)
        cli("missing-pin", ["show", "rules", "fixture", "--version", "1", "--server", endpoint], 3)
        cli("unknown-version", ["show", "rules", "fixture", "--version", "9", "--expected-digest", pins["rules"], "--server", endpoint], 3)
        status, ambiguous = request(endpoint, "GET", "/registry/rules/fixture")
        assert status == 409 and ambiguous["code"] == "RESOURCE_VERSION_REQUIRED"
        (OUT / "real-version-ambiguity.json").write_text(json.dumps({"httpStatus": status, "response": ambiguous}))
        assert fault("server-version-ambiguity", status, ambiguous, show)["code"] == "RESOURCE_VERSION_REQUIRED"
        _, audit_after = request(endpoint, "GET", "/audit")
        assert audit_before["events"] == audit_after["events"], "registry reads must not append audit"
        checks.append("audit-unchanged")
        # Fault injection tests never substitute for the real-server success evidence above.
        good = shown["registry"]
        good["resource"]["type"] = "rules"
        good["resource"]["digest"] = pins["rules"]
        for label, field, value in [("tampered-digest", "digest", "sha256:" + "f" * 64),
                                    ("malformed-digest", "digest", "sha256:bad"),
                                    ("wrong-version", "version", "2"), ("wrong-id", "id", "other"),
                                    ("wrong-type", "type", "standards"), ("source-leak", "source", "not-for-output")]:
            body = copy.deepcopy(good)
            body["resource"][field] = value
            fault(label, 200, body, show)
        for label, field, value in [("wrong-schema", "schemaVersion", "asset-registry-v2"),
                                    ("wrong-api", "apiVersion", "rulefactory.io/v2"),
                                    ("authority-escalation", "authority", "PASS"),
                                    ("wrong-correlation", "requestId", "other")]:
            body = copy.deepcopy(good)
            body[field] = value
            fault(label, 200, body, show)
        bad_page = copy.deepcopy(page["registry"])
        bad_page["items"][0]["type"] = "rules"
        bad_page["hasMore"] = "true"
        fault("bad-pagination", 200, bad_page, ["list", "rules"])
        fault("server-500", 500, {"code": "INTERNAL_ERROR", "message": "not-for-output"}, show)
        fault("redirect", 302, {}, show)
        fault("timeout", 200, {}, ["list", "rules", "--timeout-ms", "20"], delay=0.1)
        for label, address in [("nonloopback", "http://192.0.2.1:80/v1"),
                               ("hostname", "http://localhost:80/v1"),
                               ("userinfo", "http://user@127.0.0.1:80/v1"),
                               ("query-url", endpoint + "?x=1"), ("fragment-url", endpoint + "#fragment"),
                               ("wrong-base", endpoint + "/other")]:
            cli(label, ["list", "rules", "--server", address], 3)
        cli("no-endpoint", ["list", "rules"], 3)
        cli("unsupported-type", ["list", "releases", "--server", endpoint], 3)
        cli("duplicate-option", ["list", "rules", "--server", endpoint, "--server", endpoint], 3)
        cli("oversized-limit", ["list", "rules", "--server", endpoint, "--limit", "257"], 3)
        (OUT / "pins.json").write_text(json.dumps(pins, indent=2))
    finally:
        server.terminate()
        server.wait(timeout=5)
# Keep a port reserved but not listening: deterministic refusal without external networking.
with socket.socket() as offline:
    offline.bind(("127.0.0.1", 0))
    assert cli("unreachable", ["list", "rules", "--server", f"http://127.0.0.1:{offline.getsockname()[1]}/v1", "--timeout-ms", "100"], 3)["code"] == "REGISTRY_UNAVAILABLE"
(OUT / "summary.json").write_text(json.dumps({"checks": checks, "count": len(checks), "authority": "UNKNOWN"}, indent=2))
print(f"registry-client-check: {len(checks)} checks passed; evidence={OUT}")
