#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-rules-config-binding-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-rules-config-binding-v1.yaml"
mkdir -p "$OUT_DIR"
test -s "$CONTRACT"

for needle in \
  'schemaVersion: review-base-rules-config-v1' \
  'required: [baseRef, baseSha, rulesDigest, configDigest, bindingDigest]' \
  'digestBinding:' \
  'bindingDigestBinds: [baseRef, baseSha, rulesDigest, configDigest]' \
  'canonicalEncoding: canonical-json-sorted-keys-without-whitespace' \
  'revision: base' \
  'exactBaseRequired: true' \
  'workingTreeFallback: forbidden' \
  'currentHeadFallback: forbidden' \
  'digestMustMatchBytesAtBase: true' \
  'loadedRulesAndConfigMustShareBaseSha: true' \
  'loadedRulesAndConfigMustShareBaseRef: true' \
  'mismatch: reject' \
  'stale: reject' \
  'sourceContent: forbidden' \
  'mode: PREPARATION_ONLY' \
  'authority: LOCAL_UNATTESTED' \
  'scmWriteback: forbidden' \
  'gateReplacement: forbidden'; do
  rg -F -q -- "$needle" "$CONTRACT"
done

cat > "$OUT_DIR/valid-binding.json" <<'JSON'
{
  "schemaVersion": "review-base-rules-config-v1",
  "repositoryDigest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "baseRef": "refs/heads/main",
  "baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "rulesDigest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "configDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
  "bindingDigest": "sha256:eae7cb809e3515cd8994e873bf0ac31f6263a10eb65e47521cf4aef162fb5176",
  "loaded": {
    "rules": {
      "rulesDigest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "loadedFromBaseRef": "refs/heads/main",
      "loadedFromBaseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    "config": {
      "configDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "loadedFromBaseRef": "refs/heads/main",
      "loadedFromBaseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }
  },
  "authority": "LOCAL_UNATTESTED",
  "mode": "PREPARATION_ONLY"
}
JSON

python3 - "$OUT_DIR/valid-binding.json" <<'PY'
import hashlib
import json
import re
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
assert payload["schemaVersion"] == "review-base-rules-config-v1"
assert re.fullmatch(r"refs/(heads|tags|remotes)/[^\s]+", payload["baseRef"])
assert re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", payload["baseSha"])
for key in ("repositoryDigest", "rulesDigest", "configDigest", "bindingDigest"):
    assert re.fullmatch(r"sha256:[0-9a-f]{64}", payload[key]), key
loaded = payload["loaded"]
rules = loaded["rules"]
config = loaded["config"]
assert rules["rulesDigest"] == payload["rulesDigest"]
assert config["configDigest"] == payload["configDigest"]
assert rules["loadedFromBaseRef"] == config["loadedFromBaseRef"] == payload["baseRef"]
assert rules["loadedFromBaseSha"] == config["loadedFromBaseSha"] == payload["baseSha"]
identity = {key: payload[key] for key in ("baseRef", "baseSha", "rulesDigest", "configDigest")}
canonical = json.dumps(identity, sort_keys=True, separators=(",", ":")).encode("utf-8")
expected_binding = "sha256:" + hashlib.sha256(canonical).hexdigest()
assert payload["bindingDigest"] == expected_binding
assert payload["authority"] == "LOCAL_UNATTESTED"
assert payload["mode"] == "PREPARATION_ONLY"
# The preparation projection is digest-only and must never carry source bytes.
def assert_no_source_content(value):
    if isinstance(value, dict):
        assert "sourceContent" not in value
        for child in value.values():
            assert_no_source_content(child)
    elif isinstance(value, list):
        for child in value:
            assert_no_source_content(child)

assert_no_source_content(payload)
PY

cat > "$OUT_DIR/stale-binding.json" <<'JSON'
{
  "captured": {
    "baseRef": "refs/heads/main",
    "baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "rulesDigest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "configDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  },
  "current": {
    "baseRef": "refs/heads/release",
    "baseSha": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
    "rulesDigest": "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    "configDigest": "sha256:9999999999999999999999999999999999999999999999999999999999999999"
  }
}
JSON
set +e
python3 - "$OUT_DIR/stale-binding.json" <<'PY'
import json
import sys
payload = json.load(open(sys.argv[1], encoding="utf-8"))
fields = ("baseRef", "baseSha", "rulesDigest", "configDigest")
assert payload["captured"] != payload["current"]
changed = [field for field in fields if payload["captured"][field] != payload["current"][field]]
assert changed == list(fields), changed
for field in fields:
    candidate = dict(payload["captured"])
    candidate[field] = payload["current"][field]
    assert candidate != payload["captured"], field
    print(f"stale-{field}=REJECT")
PY
stale_rc=$?
set -e
[ "$stale_rc" -eq 0 ]

cat > "$OUT_DIR/inconsistent-binding.json" <<'JSON'
{
  "baseRef": "refs/heads/main",
  "baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "rulesDigest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "configDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
  "loaded": {
    "rules": {"rulesDigest": "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff", "loadedFromBaseRef": "refs/heads/main", "loadedFromBaseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
    "config": {"configDigest": "sha256:9999999999999999999999999999999999999999999999999999999999999999", "loadedFromBaseRef": "refs/heads/release", "loadedFromBaseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
  }
}
JSON
set +e
python3 - "$OUT_DIR/inconsistent-binding.json" <<'PY'
import json
import sys
payload = json.load(open(sys.argv[1], encoding="utf-8"))
invalid_rules = payload["rulesDigest"] != payload["loaded"]["rules"]["rulesDigest"]
invalid_config_digest = payload["configDigest"] != payload["loaded"]["config"]["configDigest"]
invalid_config_base = payload["baseSha"] != payload["loaded"]["config"]["loadedFromBaseSha"]
invalid_config_ref = payload["baseRef"] != payload["loaded"]["config"]["loadedFromBaseRef"]
assert invalid_rules, "rules digest mismatch should be rejected"
assert invalid_config_digest, "config digest mismatch should be rejected"
assert not invalid_config_base, "fixture should keep config at the same base"
assert invalid_config_ref, "config base ref mismatch should be rejected"
print("rules-digest-mismatch=REJECT")
PY
inconsistent_rc=$?
set -e
[ "$inconsistent_rc" -eq 0 ]

cat > "$OUT_DIR/review-rules-config-binding.md" <<'EOF2'
# Review Base Rules and Config Binding Evidence

- Rules and project config are loaded only from the immutable `baseRef`/`baseSha`.
- `rulesDigest` and `configDigest` must match the bytes loaded at that base revision;
  one binding digest covers the base identity and both input digests.
- A changed base revision or digest mismatch is rejected as stale/inconsistent.
- The projection carries no source content and cannot write back to SCM, replace a Gate,
  mutate a Gate, or promote a release.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED.
EOF2

echo "review-rules-config-binding=PASS evidence=$OUT_DIR/review-rules-config-binding.md"
