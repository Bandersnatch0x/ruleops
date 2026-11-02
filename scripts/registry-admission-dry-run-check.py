#!/usr/bin/env python3
import json, pathlib, re, sys

def fail(m): raise SystemExit(f"FAIL {m}")
p=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else pathlib.Path("docs/research/registry-fixtures/admission-dry-run.json")
try: d=json.loads(p.read_text())
except Exception as e: fail(f"invalid-json {e}")
for k in ("schema","mode","registry","repository","version","artifactDigest","signatureDigest","revocationSnapshot","rollbackRef","authority"):
    if not d.get(k): fail(f"missing-{k}")
if d["schema"] != "registry-admission-v1": fail("schema")
if d["mode"] not in {"PROPOSED","DRY_RUN"}: fail("mode")
if not re.fullmatch(r"https://[^ ]+", d["registry"]): fail("registry-url")
for k in ("artifactDigest","signatureDigest","revocationSnapshot"):
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", d[k]): fail(k)
if d["authority"] != "FORMAT_ONLY": fail("authority")
if not d["rollbackRef"].startswith("registry://"): fail("rollback-ref")
print(f"registry-admission=PASS mode={d['mode']} repository={d['repository']} version={d['version']}")
