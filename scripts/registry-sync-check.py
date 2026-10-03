#!/usr/bin/env python3
"""Bounded signed loopback sync acceptance; output directory must be explicit."""
import base64
import copy
import http.server
import json
import os
import ssl
from pathlib import Path
import subprocess
import sys
import threading
import urllib.parse

REPO = Path(__file__).resolve().parents[1]

def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: registry-sync-check.py OUTPUT_DIR")
    out = Path(sys.argv[1]).resolve()
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ)
    build = subprocess.run(["cargo", "build", "-q", "-p", "rf-cli", "--bin", "rf-cli", "--example", "make_delivery_fixture"], cwd=REPO, env=env, capture_output=True, timeout=120)
    (out / "build.log").write_bytes(build.stdout + build.stderr)
    assert build.returncode == 0, "build failed: see build.log"
    root = out / "workspace"
    (root / ".ruleops").mkdir(parents=True)
    for name in ["backend", "resolver", "ruleset.xml"]:
        (out / name).write_text("<ruleset name='fixture'/>" if name.endswith("xml") else "test-only-runtime-asset")
    descriptor_file = root / ".ruleops/delivery.json"
    fixture = subprocess.run([str(REPO / "target/debug/examples/make_delivery_fixture"), str(root), str(out / "ruleset.xml"), str(out / "backend"), str(out / "resolver"), str(descriptor_file), str(root / ".ruleops/delivery-execution.lock"), "connected"], cwd=REPO, env=env, capture_output=True, timeout=20)
    (out / "fixture.log").write_bytes(fixture.stdout + fixture.stderr)
    assert fixture.returncode == 0, "fixture failed"
    descriptor = json.loads(descriptor_file.read_text())
    pin = descriptor["registryPin"]
    cache = root / descriptor["cacheRoot"] / "sha256" / descriptor["descriptor"]["bundleDigest"].removeprefix("sha256:")
    bundle = cache.read_bytes()
    assert pin["storeDigest"] != pin["bundleDigest"], "fixture must not conflate store and bundle digests"
    key = bytes.fromhex("302e020100300506032b657004220420") + bytes([100]) * 32
    public = subprocess.run(["openssl", "pkey", "-inform", "DER", "-pubout", "-outform", "DER"], input=key, capture_output=True, check=True, timeout=10).stdout[-32:].hex()
    env["RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX"] = public
    state = {"registryDigest": pin["storeDigest"], "bundle": bundle, "deliveryCalls": 0}
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass
        def do_GET(self):
            if state.get("tlsAuth"):
                assert self.headers.get("Authorization") == "Bearer " + env["RF_SYNC_TEST_TOKEN"]
            parsed = urllib.parse.urlsplit(self.path)
            if parsed.path == "/v1/registry/rules/" + pin["id"]:
                assert urllib.parse.parse_qs(parsed.query) == {"version": ["1.0.0"]}
                body = {"apiVersion": "rulefactory.io/v1", "schemaVersion": "asset-registry-v1", "authority": "UNKNOWN", "requestId": "registry-read", "resource": {"type": "rules", "id": pin["id"], "version": "1.0.0", "state": "RELEASED", "digest": state["registryDigest"]}}
            elif parsed.path == "/v1/rule-bundles/" + pin["bundleDigest"] + "/delivery":
                state["deliveryCalls"] += 1
                body = {"apiVersion": "rulefactory.io/v1", "kind": "RuleBundleDelivery", "schemaVersion": "rule-bundle-delivery-v1", "descriptor": descriptor["descriptor"], "verification": descriptor["verification"]["evidence"], "bundleBytesBase64": base64.b64encode(state["bundle"]).decode()}
            else:
                self.send_error(404)
                return
            wire = json.dumps(body).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(wire)))
            self.end_headers()
            self.wfile.write(wire)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    endpoint = f"http://127.0.0.1:{server.server_port}/v1"
    descriptor["endpoint"] = endpoint
    def write(value):
        descriptor_file.write_text(json.dumps(value))
    write(descriptor)
    command = [str(REPO / "rf"), "registry", "sync", "rules", pin["id"], "--version", "1.0.0", "--expected-digest", pin["storeDigest"], "--delivery-descriptor", str(descriptor_file), "--repo-root", str(root), "--server", endpoint, "--timeout-ms", "2000"]
    results = []
    def run(name, expected=None, args=None, environment=None):
        result = subprocess.run(args or command, cwd=REPO, env=environment or env, capture_output=True, timeout=20)
        (out / (name + ".stdout.json")).write_bytes(result.stdout)
        (out / (name + ".stderr.log")).write_bytes(result.stderr)
        receipt = json.loads(result.stdout)
        assert receipt["authority"] == "UNKNOWN", receipt
        if expected:
            assert result.returncode == 3 and receipt["status"] == "UNKNOWN" and receipt["code"] == expected, (name, result.returncode, receipt)
        else:
            assert result.returncode == 0 and receipt["status"] == "CACHED", (name, result.returncode, receipt)
            assert receipt["artifactKind"] == "SignedRuleBundle" and receipt["offlineAvailable"] is True
            assert receipt["destination"] == str(cache)
            assert receipt["overwritePolicy"] == "IMMUTABLE_DIGEST_ADDRESSED"
            assert receipt["bundleDigest"] == pin["bundleDigest"]
        with (out / "rc.txt").open("a") as log:
            log.write(f"{name} rc={result.returncode} expected={3 if expected else 0}\n")
        results.append(name)
        return receipt
    try:
        cache.unlink()  # Only the exact generated fixture object; force a real cold fetch.
        run("cold-fetch")
        assert cache.read_bytes() == bundle and state["deliveryCalls"] == 1
        before = cache.stat().st_mtime_ns
        run("warm-reverify")
        assert state["deliveryCalls"] == 1 and cache.stat().st_mtime_ns == before
        bad_env = dict(env, RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX="00" * 32)
        run("wrong-root-warm", "REGISTRY_DELIVERY_INVALID", environment=bad_env)
        state["registryDigest"] = "sha256:" + "01" * 32
        run("registry-pin-mismatch", "REGISTRY_DIGEST_MISMATCH")
        state["registryDigest"] = pin["storeDigest"]
        for name, mutate, code in [
            ("missing-mapping", lambda d: d.pop("registryPin"), "REGISTRY_SIGNED_MAPPING_REQUIRED"),
            ("mapping-version-tamper", lambda d: d["registryPin"].update(version="2.0.0"), "REGISTRY_DELIVERY_INVALID"),
            ("mapping-provenance-tamper", lambda d: d["registryPin"].update(provenance="attacker"), "REGISTRY_DELIVERY_INVALID"),
            ("mapping-bundle-tamper", lambda d: d["registryPin"].update(bundleDigest=pin["storeDigest"]), "REGISTRY_DELIVERY_INVALID"),
            ("endpoint-mismatch", lambda d: d.update(endpoint="http://127.0.0.1:9/v1"), "REGISTRY_DELIVERY_IDENTITY_MISMATCH"),
            ("missing-lock", lambda d: d.pop("executionLockPath"), "REGISTRY_EXECUTION_LOCK_REQUIRED"),
            ("compatibility-mismatch", lambda d: d.update(expectedCompatibilityIdentity="other"), "REGISTRY_DELIVERY_INVALID"),
        ]:
            changed = copy.deepcopy(descriptor)
            mutate(changed)
            write(changed)
            run(name, code)
            assert cache.read_bytes() == bundle and cache.stat().st_mtime_ns == before
        write(descriptor)
        run("changed-version-needs-signed-mapping", "REGISTRY_MAPPING_PIN_MISMATCH", args=["latest" if x == "1.0.0" else x for x in command])
        cache.unlink()
        changed = copy.deepcopy(descriptor)
        envelope = changed["verification"]["bundle"]["signedManifest"]
        envelope["signatures"][0]["sig"] = base64.b64encode(bytes(64)).decode()
        write(changed)
        run("bundle-signature-before-cache", "REGISTRY_DELIVERY_UNTRUSTED")
        assert not cache.exists() and state["deliveryCalls"] == 1
        write(descriptor)
        state["bundle"] = b"tampered"
        run("tampered-delivery", "REGISTRY_DELIVERY_UNAVAILABLE")
        assert not cache.exists()
        state["bundle"] = bundle
        run("recovery-cold-fetch")
        cache.write_bytes(b"corrupt-local-object")
        run("corrupt-cache-no-overwrite", "REGISTRY_CACHE_INVALID")
        assert cache.read_bytes() == b"corrupt-local-object"
        cache.unlink()
        tls = out / "tls"
        tls.mkdir()
        def openssl(*args):
            subprocess.run(["openssl", *args], cwd=tls, capture_output=True, check=True, timeout=15)
        openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=Sync fixture CA", "-keyout", "ca.key", "-out", "ca.pem")
        openssl("req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=localhost", "-keyout", "server.key", "-out", "server.csr")
        (tls / "server.ext").write_text("subjectAltName=IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n")
        openssl("x509", "-req", "-in", "server.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial", "-days", "1", "-extfile", "server.ext", "-out", "server.pem")
        secure = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(tls / "server.pem", tls / "server.key")
        secure.socket = context.wrap_socket(secure.socket, server_side=True)
        secure_worker = threading.Thread(target=secure.serve_forever, daemon=True)
        secure_worker.start()
        approved = {"schemaVersion": "rf-connection-profile-v1", "approved": True, "endpoint": f"https://127.0.0.1:{secure.server_port}/v1", "provenance": "test-fixture:approved-loopback-only", "bearerTokenEnv": "RF_SYNC_TEST_TOKEN", "caCertificateFile": "tls/ca.pem"}
        profile_path = out / "connection.json"
        profile_path.write_text(json.dumps(approved))
        env["RF_SYNC_TEST_TOKEN"] = "test-only-" + os.urandom(16).hex()
        state["tlsAuth"] = True
        descriptor["endpoint"] = approved["endpoint"]
        write(descriptor)
        remote = command.copy()
        index = remote.index("--server")
        remote[index:index + 2] = ["--connection-profile", str(profile_path)]
        try:
            run("approved-profile-cold-fetch", args=remote)
            assert cache.read_bytes() == bundle
            run("approved-profile-warm-reverify", args=remote)
            approved["approved"] = False
            profile_path.write_text(json.dumps(approved))
            run("unapproved-profile-rejected", "REGISTRY_CONNECTION_PROFILE_INVALID", args=remote)
        finally:
            secure.shutdown()
            secure.server_close()
            secure_worker.join(timeout=3)
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=3)
    (out / "summary.json").write_text(json.dumps({"status": "PASS", "checks": results, "realRemoteEndpointTested": False}, indent=2))
    print(f"registry-sync=PASS checks={len(results)} output={out}")

if __name__ == "__main__":
    main()
