#!/usr/bin/env python3
"""External ADVISORY GitHub transport; frozen contracts are checked by erf-status."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request

CONTEXT = "erf-advisory"
API = "https://api.github.com"


class DeliveryError(Exception):
    def __init__(self, outcome, reason):
        self.outside = outcome
        self.reason = reason


def reject(reason):
    raise DeliveryError("REJECTED", reason)


def unknown(reason):
    raise DeliveryError("UNKNOWN", reason)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def digest(value):
    return "sha256:" + hashlib.sha256(canonical(value)).hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def read_json(path):
    return json.loads(Path(path).read_text(), object_pairs_hook=unique_object)


def contract(checker, publication, deployment, previous=None):
    try:
        checked = subprocess.run([str(checker)], input=canonical({
            "publication": publication, "deployment": deployment,
            "previous": previous, "now": int(time.time()),
        }), capture_output=True, timeout=15, check=False)
        result = json.loads(checked.stdout)
    except (OSError, subprocess.TimeoutExpired, ValueError):
        unknown("contract-check-unavailable")
    if checked.returncode == 2:
        # The checker emits fixed contract errors, never input values or credentials.
        reject(result["reason"])
    if checked.returncode != 0 or "receipt" not in result:
        unknown("contract-check-failed")
    return result["receipt"]


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        reject("provider-redirect-refused")


class GitHub:
    def __init__(self, token):
        self.token = token
        self.opener = urllib.request.build_opener(NoRedirect)

    def __call__(self, method, url, body=None):
        headers = {"Authorization": "Bearer " + self.token,
                   "Accept": "application/vnd.github+json",
                   "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "erf-advisory-adapter"}
        if body is not None:
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(url, data=None if body is None else canonical(body),
                                         headers=headers, method=method)
        try:
            with self.opener.open(request, timeout=20) as response:
                if response.status != (201 if method == "POST" else 200):
                    unknown("unexpected-provider-response")
                return json.load(response)
        except urllib.error.HTTPError as error:
            if error.code in (408, 429) or error.code >= 500 or (
                    error.code == 403 and (error.headers.get("X-RateLimit-Remaining") == "0"
                                          or error.headers.get("Retry-After"))):
                unknown("provider-unavailable-http-" + str(error.code))
            reject("provider-rejected-http-" + str(error.code))
        except (urllib.error.URLError, TimeoutError, socket.timeout, OSError, ValueError):
            unknown("provider-transport-or-response-unknown")


def target(publication, deployment):
    if deployment["provider"] != "github":
        reject("provider-must-be-github")
    if deployment["credentialRef"] != "secretref://env/ERF_STATUS_TOKEN":
        reject("unresolved-credential-reference")
    match = re.fullmatch(r"https://api\.github\.com/repos/([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)/statuses/([0-9a-f]{40})",
                         deployment["endpoint"])
    if not in_scope(match):
        reject("endpoint-outside-github-status-boundary")
    repo, sha = match.groups()
    if repo != publication["repository"] or sha != publication["commit"]:
        reject("deployment-target-identity-mismatch")
    key_digest = hashlib.sha256(deployment["idempotencyKey"].encode()).hexdigest()
    body = {"state": {"success": "success", "failure": "failure", "action-required": "error"}[publication["state"]],
            "context": CONTEXT, "description": publication["publicationDigest"],
            "target_url": f"https://github.com/{repo}/commit/{sha}#erf-advisory-{key_digest}"}
    return repo, sha, body


def in_scope(match):
    return match is not None and all(part not in (".", "..") for part in match[1].split("/"))


def latest_status(transport, repo, sha):
    # GitHub lists newest first. Bound work rather than silently ignoring older pages.
    for page in range(1, 11):
        statuses = transport("GET", f"{API}/repos/{repo}/commits/{sha}/statuses?per_page=100&page={page}")
        if not isinstance(statuses, list) or any(not isinstance(s, dict) for s in statuses):
            unknown("invalid-provider-status-list")
        for status in statuses:
            if status.get("context") == CONTEXT:
                return status
        if len(statuses) < 100:
            return None
    unknown("provider-history-pagination-limit")


def matches(status, expected):
    return (isinstance(status, dict) and type(status.get("id")) is int and status["id"] > 0
            and all(status.get(key) == value for key, value in expected.items()))


def provider_evidence(status):
    return {key: status[key] for key in ("id", "state", "context", "description", "target_url")}


def save(path, ledger):
    temporary = path.with_suffix(".tmp")
    with temporary.open("w") as stream:
        json.dump(ledger, stream, sort_keys=True)
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)
    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def deliver(publication, deployment, checker, state_dir, token, transport=None):
    receipt = {"apiVersion": "rulefactory.io/v1", "kind": "GitHubStatusAdapterDelivery",
               "schemaVersion": "github-status-adapter-delivery-v1", "authority": "ADVISORY",
               "protectedBranchAuthority": False, "scmWritten": False, "providerResponse": None, "contractReceipt": None}
    try:
        receipt["contractReceipt"] = contract(checker, publication, deployment)
        repo, sha, body = target(publication, deployment)
        if not token or any(not 33 <= ord(char) <= 126 for char in token):
            reject("missing-or-invalid-deployment-credential")
        if transport is None:
            transport = GitHub(token)
        state_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        # ponytail: one local ledger lock; use a transactional shared store before multi-host deployment.
        with (state_dir / "ledger.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            path = state_dir / "ledger.json"
            ledger = read_json(path) if path.exists() else {"targets": {}, "keys": {}}
            if (not isinstance(ledger, dict) or set(ledger) != {"targets", "keys"}
                    or not isinstance(ledger["targets"], dict) or not isinstance(ledger["keys"], dict)):
                unknown("invalid-local-ledger-shape")
            identity = deployment["endpoint"]
            key = deployment["idempotencyKey"]
            bound = ledger["keys"].get(key)
            if bound is not None and bound != deployment["deploymentDigest"]:
                reject("idempotency-key-conflict")
            current = ledger["targets"].get(identity)
            previous = None if current is None else current["publication"]
            receipt["contractReceipt"] = contract(checker, publication, deployment, previous)
            if current and current["phase"] == "PENDING" and current["deployment"] != deployment:
                unknown("previous-delivery-unresolved")
            latest = latest_status(transport, repo, sha)
            if matches(latest, body):
                if current is None or current["deployment"] != deployment:
                    # Remote fields cannot recover the full monotonic identity or request binding.
                    reject("remote-replay-without-trusted-ledger")
                current.update(phase="CONFIRMED", provider=provider_evidence(latest))
                save(path, ledger)
                receipt.update(outcome="IDEMPOTENT", scmWritten=True, providerResponse=provider_evidence(latest))
            else:
                if current is not None:
                    if current["phase"] == "CONFIRMED":
                        if not matches(latest, current["provider"]) or latest["id"] != current["provider"]["id"]:
                            reject("remote-status-diverged-from-ledger")
                        if publication == current["publication"]:
                            reject("same-publication-different-request")
                    else:
                        # Do not retry POST after a timeout: GitHub has no idempotency-key API.
                        unknown("pending-write-not-yet-confirmed")
                elif latest is not None:
                    reject("remote-context-without-trusted-ledger")
                # Recheck freshness after any network wait, before storing intent and sending.
                receipt["contractReceipt"] = contract(checker, publication, deployment, previous)
                ledger["keys"][key] = deployment["deploymentDigest"]
                ledger["targets"][identity] = {"publication": publication, "deployment": deployment,
                                                "phase": "PENDING"}
                save(path, ledger)
                status = transport("POST", deployment["endpoint"], body)
                if not matches(status, body):
                    unknown("provider-write-not-confirmed")
                ledger["targets"][identity].update(phase="CONFIRMED", provider=provider_evidence(status))
                save(path, ledger)
                receipt.update(outcome="ACCEPTED", scmWritten=True, providerResponse=provider_evidence(status))
        receipt["status"] = "PROVIDER_CONFIRMED"
    except DeliveryError as error:
        receipt.update(outcome=error.outside, reason=error.reason, status="NOT_CONFIRMED",
                       scmWritten=False)
    except (OSError, ValueError):
        receipt.update(outcome="UNKNOWN", reason="local-ledger-or-input-unavailable",
                       status="NOT_CONFIRMED", scmWritten=False)
    receipt["receiptDigest"] = digest(receipt)
    return receipt


def main():
    os.umask(0o077)
    # Remove it before invoking the validator. The credential exists only in memory and HTTPS headers.
    token = os.environ.pop("ERF_STATUS_TOKEN", "")
    try:
        publication, deployment = (read_json(path) for path in sys.argv[1:3])
    except (OSError, ValueError):
        receipt = {"apiVersion": "rulefactory.io/v1", "kind": "GitHubStatusAdapterDelivery",
                   "schemaVersion": "github-status-adapter-delivery-v1", "authority": "ADVISORY",
                   "protectedBranchAuthority": False, "outcome": "REJECTED", "status": "NOT_CONFIRMED",
                   "reason": "unreadable-or-invalid-json", "scmWritten": False, "providerResponse": None, "contractReceipt": None}
        receipt["receiptDigest"] = digest(receipt)
    else:
        state = Path(__file__).resolve().parents[1] / ".scratch/scm-status-adapter-state"
        receipt = deliver(publication, deployment, Path(sys.argv[3]), state, token)
    print(json.dumps(receipt, sort_keys=True))
    return {"ACCEPTED": 0, "IDEMPOTENT": 0, "REJECTED": 2, "UNKNOWN": 3}[receipt["outcome"]]


if __name__ == "__main__":
    sys.exit(main())
