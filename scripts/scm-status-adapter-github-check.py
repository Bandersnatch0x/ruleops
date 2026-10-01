#!/usr/bin/env python3
"""Offline regression checks: real contract checker, explicitly fake provider."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import urllib.error
import tempfile
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("adapter", ROOT / "scripts/scm-status-adapter-github.py")
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)
CHECKER = ROOT / "target/debug/erf-github-contract-check"


def seal(value, field):
    value.pop(field, None)
    value[field] = adapter.digest(value)
    return value


def fixtures(sha="a" * 40):
    now = int(time.time())
    publication = seal({"apiVersion": "rulefactory.io/v1", "kind": "StatusPublication",
        "schemaVersion": "status-publication-v1", "status": "READY_FOR_ADAPTER",
        "repository": "Bandersnatch0x/ruleops", "commit": sha, "policyDigest": "a" * 64,
        "attestationSubject": "sha256:" + "b" * 64, "decision": "INCONCLUSIVE", "state": "action-required",
        "runSequence": 1, "issuedAtEpochSecs": now - 10, "expiresAtEpochSecs": now + 3600,
        "externalAdapterRequired": True, "scmWritten": False}, "publicationDigest")
    deployment = seal({"apiVersion": "rulefactory.io/v1", "kind": "StatusAdapterDeployment",
        "schemaVersion": "status-adapter-deployment-v1", "status": "READY_FOR_EXTERNAL_CONNECTOR",
        "provider": "github", "endpoint": f"https://api.github.com/repos/Bandersnatch0x/ruleops/statuses/{sha}",
        "credentialRef": "secretref://env/ERF_STATUS_TOKEN", "permissionScope": ["status:write"],
        "publicationDigest": publication["publicationDigest"], "requestId": f"advisory-canary-{now}",
        "idempotencyKey": f"advisory-canary-{now}", "externalConnectorRequired": True, "scmWritten": False},
        "deploymentDigest")
    return publication, deployment


class FakeProvider:
    def __init__(self):
        self.statuses = []
        self.posts = 0
        self.lose_response = False

    def __call__(self, method, url, body=None):
        if method == "GET":
            return self.statuses
        self.posts += 1
        status = dict(body, id=self.posts)
        self.statuses.insert(0, status)
        if self.lose_response:
            adapter.unknown("test-lost-response-after-write")
        return status


def main():
    os.environ.pop("ERF_STATUS_TOKEN", None)
    build = subprocess.run(["bash", str(ROOT / "scripts/scm-status-adapter-github.sh"), "/nonexistent", "/nonexistent"],
                           capture_output=True, timeout=120)
    assert build.returncode == 2, (build.returncode, build.stderr.decode())
    assert json.loads(build.stdout)["outcome"] == "REJECTED"
    results = []
    with tempfile.TemporaryDirectory(prefix="github-adapter-", dir=ROOT / ".scratch") as directory:
        state = Path(directory)
        p, d = fixtures()
        provider = FakeProvider()

        def run(pub=p, dep=d, transport=provider, store=state):
            return adapter.deliver(pub, dep, CHECKER, store, "test-only", transport)

        first, replay = run(), run()
        assert first["outcome"] == "ACCEPTED" and replay["outcome"] == "IDEMPOTENT"
        assert provider.posts == 1 and first["providerResponse"]["state"] == "error"
        assert first["contractReceipt"]["scmWritten"] is False and first["scmWritten"] is True
        assert replay["contractReceipt"]["outcome"] == "IDEMPOTENT"
        results.append("accepted + replay: one POST; INCONCLUSIVE -> error; layered receipts")
        for label, pub, dep in [
            ("publication-digest", dict(p, publicationDigest="sha256:" + "0" * 64), d),
            ("deployment-digest", p, dict(d, deploymentDigest="sha256:" + "0" * 64)),
            ("expired", seal(dict(p, expiresAtEpochSecs=int(time.time())), "publicationDigest"), d),
            ("broad-scope", p, seal(dict(d, permissionScope=["repo"]), "deploymentDigest")),
            ("wrong-target", p, seal(dict(d, endpoint=d["endpoint"].replace("a" * 40, "c" * 40)), "deploymentDigest")),
            ("credential-exfiltration", p, seal(dict(d, endpoint=d["endpoint"].replace("api.github.com", "evil.invalid")), "deploymentDigest")),
            ("unknown-field", dict(p, token="never-a-real-token"), d),
            ("idempotency-conflict", p, seal(dict(d, requestId="different"), "deploymentDigest")),
        ]:
            result = run(pub, dep)
            assert result["outcome"] == "REJECTED" and result["scmWritten"] is False, (label, result)
            assert provider.posts == 1
            results.append(label + ": REJECTED; no POST")
        newer = seal(dict(p, runSequence=2, issuedAtEpochSecs=p["issuedAtEpochSecs"] + 1,
                          attestationSubject="sha256:" + "c" * 64), "publicationDigest")
        newer_d = seal(dict(d, publicationDigest=newer["publicationDigest"], requestId="run2", idempotencyKey="run2"), "deploymentDigest")
        assert run(newer, newer_d)["outcome"] == "ACCEPTED"
        assert run()["outcome"] == "REJECTED" and provider.posts == 2
        results.append("newer run accepted; old run rejected")
        orphan = run(newer, newer_d, store=state / "lost-ledger")
        assert orphan["outcome"] == "REJECTED"
        results.append("missing ledger cannot infer trusted monotonic history: REJECTED")
        lost = FakeProvider()
        lost.lose_response = True
        pending = run(transport=lost, store=state / "lost-response")
        recovered = run(transport=lost, store=state / "lost-response")
        assert pending["outcome"] == "UNKNOWN" and pending["scmWritten"] is False
        assert recovered["outcome"] == "IDEMPOTENT" and lost.posts == 1
        results.append("lost POST response: UNKNOWN; GET recovery: IDEMPOTENT; no duplicate POST")
        class HttpFailure:
            def __init__(self, code):
                self.code = code
            def open(self, request, timeout):
                raise urllib.error.HTTPError(request.full_url, self.code, "test", {}, None)
        for code, expected in [(401, "REJECTED"), (403, "REJECTED"), (404, "REJECTED"),
                               (429, "UNKNOWN"), (503, "UNKNOWN")]:
            http = adapter.GitHub("test-only")
            http.opener = HttpFailure(code)
            result = run(transport=http, store=state / str(code))
            assert result["outcome"] == expected and result["scmWritten"] is False
            results.append(f"HTTP {code}: {expected}")
        assert adapter.deliver(p, d, CHECKER, state, "", provider)["outcome"] == "REJECTED"
        def offline(method, url, body=None):
            adapter.unknown("test-network-unavailable")
        assert run(transport=offline, store=state / "offline")["outcome"] == "UNKNOWN"
        results.append("transport failure: UNKNOWN")
    print(json.dumps({"status": "PASS", "checks": results}, indent=2))


if __name__ == "__main__":
    main()
