#!/usr/bin/env python3
"""Static dry-run attestation contract checker; no identity verification or trust grant."""
import json, pathlib, re, sys

def fail(msg): raise SystemExit(f"FAIL {msg}")
p=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else pathlib.Path("docs/research/trusted-ci-fixtures/attestation-dry-run.json")
try: d=json.loads(p.read_text())
except Exception as e: fail(f"invalid-json {e}")
req=("schema","mode","workloadIdentity","repository","workflow","commitSha","attestationSubject","issuer","policyDigest","issuedAt","expiresAt","rollbackRef")
for k in req:
    if not d.get(k): fail(f"missing-{k}")
if d["schema"] != "trusted-ci-attestation-v1": fail("schema")
if d["mode"] not in {"PROPOSED","DRY_RUN"}: fail("mode")
if not re.fullmatch(r"[0-9a-f]{40}", d["commitSha"]): fail("commit-sha")
for k in ("attestationSubject","policyDigest"):
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", d[k]): fail(k)
if not re.fullmatch(r"https://[^ ]+", d["issuer"]): fail("issuer")
if d["expiresAt"] <= d["issuedAt"]: fail("expiry-window")
if "@" not in d["rollbackRef"]: fail("rollback-ref")
print(f"trusted-ci-attestation=PASS mode={d['mode']} repository={d['repository']} commit={d['commitSha']}")
