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
import ssl
import secrets
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
# Approved transport acceptance uses ONLY a loopback TLS fixture, not an enterprise endpoint.
tls = OUT / "tls"
tls.mkdir()
run("tls-ca", ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
               "-subj", "/CN=RegistryFixtureCA", "-keyout", str(tls / "ca.key"), "-out", str(tls / "ca.pem")])
run("tls-csr", ["openssl", "req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=localhost",
                "-keyout", str(tls / "leaf.key"), "-out", str(tls / "leaf.csr")])
(tls / "leaf.ext").write_text("subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth,clientAuth\n")
run("tls-cert", ["openssl", "x509", "-req", "-in", str(tls / "leaf.csr"), "-CA", str(tls / "ca.pem"),
                 "-CAkey", str(tls / "ca.key"), "-CAcreateserial", "-days", "1", "-extfile", str(tls / "leaf.ext"),
                 "-out", str(tls / "leaf.pem")])
(tls / "client.pem").write_bytes((tls / "leaf.pem").read_bytes() + (tls / "leaf.key").read_bytes())
token = secrets.token_urlsafe(32)
os.environ["RULEOPS_REGISTRY_FIXTURE_TOKEN"] = token
page_cursor = "fixture\x001&version=one +%/中文"
requests = []
redirect = False


class SecureRegistry(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        from urllib.parse import parse_qs
        requests.append(self.path)
        assert self.headers.get("Authorization") == "Bearer " + token or self.connection.getpeercert()
        assert self.headers.get("X-Request-Id") == "registry-read"
        query = parse_qs(urlsplit(self.path).query)
        base = {"apiVersion": "rulefactory.io/v1", "schemaVersion": "asset-registry-v1",
                "authority": "UNKNOWN", "requestId": "registry-read"}
        resource = {"type": "rules", "id": "fixture", "version": "1", "state": "DRAFT", "digest": pins["rules"]}
        if urlsplit(self.path).path.endswith("/fixture"):
            data = {**base, "resource": resource}
        else:
            cursor = query.get("cursor", [""])[0]
            assert cursor in ("", page_cursor)
            data = {**base, "items": [resource] if not cursor else [], "cursor": cursor,
                    "nextCursor": page_cursor, "hasMore": not bool(cursor)}
        body = json.dumps({} if redirect else data).encode()
        self.send_response(302 if redirect else 200)
        if redirect:
            self.send_header("Location", "https://localhost:1/do-not-follow")
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(tls / "leaf.pem", tls / "leaf.key")
context.load_verify_locations(tls / "ca.pem")
context.verify_mode = ssl.CERT_OPTIONAL
with http.server.HTTPServer(("127.0.0.1", 0), SecureRegistry) as secure:
    secure.socket = context.wrap_socket(secure.socket, server_side=True)
    thread = threading.Thread(target=secure.serve_forever, kwargs={"poll_interval": 0.01})
    thread.start()
    try:
        profile = {"schemaVersion": "rf-connection-profile-v1", "approved": True,
                   "endpoint": f"https://localhost:{secure.server_port}/v1", "provenance": "loopback fixture approval only",
                   "bearerTokenEnv": "RULEOPS_REGISTRY_FIXTURE_TOKEN", "caCertificateFile": "ca.pem"}
        profile_path = tls / "connection.json"
        profile_path.write_text(json.dumps(profile))
        connection = ["--connection-profile", str(profile_path)]
        # An ambient proxy must not silently change the explicitly selected path.
        old_proxy = os.environ.get("HTTPS_PROXY")
        os.environ["HTTPS_PROXY"] = "http://127.0.0.1:1"
        try:
            secure_page = cli("approved-https-list", ["list", "rules", "--limit", "1", *connection])
        finally:
            if old_proxy is None:
                del os.environ["HTTPS_PROXY"]
            else:
                os.environ["HTTPS_PROXY"] = old_proxy
        assert secure_page["digestVerification"] == "FORMAT_ONLY"
        assert secure_page["registry"]["nextCursor"] == page_cursor
        next_page = cli("approved-https-cursor", ["list", "rules", "--limit", "1", "--cursor-json", json.dumps(page_cursor), *connection])
        assert not next_page["registry"]["hasMore"]
        assert "%00" in requests[-1] and "%26" in requests[-1]
        assert cli("approved-https-pin", [*show, *connection])["digestVerification"] == "PIN_MATCH"
        cli("approved-https-wrong-pin", ["show", "rules", "fixture", "--version", "1", "--expected-digest", "sha256:" + "0" * 64, *connection], 3)
        for label, field, value in [
            ("approval-required", "approved", False),
            ("profile-plaintext-forbidden", "endpoint", f"http://127.0.0.1:{secure.server_port}/v1"),
            ("profile-userinfo-forbidden", "endpoint", "https://user:not-for-output@localhost/v1"),
            ("profile-query-forbidden", "endpoint", "https://localhost/v1?token=not-for-output"),
            ("profile-inline-secret-forbidden", "bearerToken", "not-for-output"),
            ("profile-credential-missing", "bearerTokenEnv", "RULEOPS_UNSET_FIXTURE_TOKEN"),
            ("profile-ca-missing", "caCertificateFile", "absent-ca.pem"),
            ("profile-proxy-no-fallback", "proxy", "https://127.0.0.1:1"),
        ]:
            broken = {**profile, field: value}
            profile_path.write_text(json.dumps(broken))
            before = len(requests)
            cli(label, ["list", "rules", *connection, "--timeout-ms", "500"], 3)
            assert len(requests) == before, label
        broken = dict(profile)
        del broken["caCertificateFile"]
        profile_path.write_text(json.dumps(broken))
        cli("profile-untrusted-ca", ["list", "rules", *connection], 3)
        profile_path.write_text(json.dumps(profile))
        cli("profile-server-conflict", ["list", "rules", *connection, "--server", profile["endpoint"]], 3)
        redirect = True
        before = len(requests)
        cli("approved-https-no-redirect", ["list", "rules", *connection], 3)
        assert len(requests) == before + 1
        redirect = False
        mtls_profile = {**profile, "clientIdentityPemFile": "client.pem"}
        del mtls_profile["bearerTokenEnv"]
        profile_path.write_text(json.dumps(mtls_profile))
        assert cli("approved-mtls-pin", [*show, *connection])["digestVerification"] == "PIN_MATCH"
    finally:
        secure.shutdown()
        thread.join(timeout=3)
        assert not thread.is_alive()
        del os.environ["RULEOPS_REGISTRY_FIXTURE_TOKEN"]
# No fixture or profile validation above contacted a real remote endpoint.
(OUT / "summary.json").write_text(json.dumps({"checks": checks, "count": len(checks), "authority": "UNKNOWN", "realRemoteEndpointTested": False}, indent=2))
print(f"registry-client-check: {len(checks)} checks passed; evidence={OUT}")
