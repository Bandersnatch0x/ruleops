#!/usr/bin/env python3
"""Static, digest-bound two-person approval policy checker; no signing or authority."""
import json, pathlib, re, sys

def fail(msg): raise SystemExit(f"FAIL {msg}")
p=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else pathlib.Path("docs/research/dual-approval-fixtures/root-rotation.json")
try: d=json.loads(p.read_text())
except Exception as e: fail(f"invalid-json {e}")
for k in ("schema","operationId","target","requestedBy","requestDigest","rollbackRef","approvals"):
    if k not in d: fail(f"missing-{k}")
if d["schema"] != "dual-approval-policy-v1": fail("schema")
if not re.fullmatch(r"sha256:[0-9a-f]{64}", d["requestDigest"]): fail("request-digest")
if not isinstance(d["approvals"], list) or len(d["approvals"]) < 2: fail("approval-count")
actors=[]
for a in d["approvals"]:
    for k in ("actor","role","decision","approvedAt","expiresAt","revoked","requestDigest"):
        if k not in a: fail(f"approval-missing-{k}")
    if a["actor"] == d["requestedBy"]: fail("requester-cannot-approve")
    if a["actor"] in actors: fail("duplicate-approver")
    actors.append(a["actor"])
    if a["decision"] != "APPROVED": fail("approval-not-approved")
    if a["revoked"] is not False: fail("approval-revoked")
    if a["requestDigest"] != d["requestDigest"]: fail("approval-digest-mismatch")
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}T[^ ]+Z", a["approvedAt"]): fail("approved-at")
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}T[^ ]+Z", a["expiresAt"]): fail("expires-at")
    if a["expiresAt"] <= a["approvedAt"]: fail("approval-expired-window")
if not d["rollbackRef"]: fail("rollback-ref")
print(f"dual-approval=PASS operation={d['operationId']} approvers={len(actors)} target={d['target']}")
