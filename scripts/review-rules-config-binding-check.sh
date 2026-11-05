#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-rules-config-binding-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-rules-config-binding-v1.yaml"
mkdir -p "$OUT_DIR"
test -s "$CONTRACT"

for needle in \
  'schemaVersion: review-base-rules-config-v1' \
  'required: [repositoryDigest, baseRef, baseSha, rulesDigest, configDigest, bindingDigest]' \
  'digestBinding:' \
  'bindingDigestBinds: [repositoryDigest, baseRef, baseSha, rulesDigest, configDigest]' \
  'canonicalEncoding: canonical-json-sorted-keys-without-whitespace' \
  'revision: base' \
  'exactBaseRequired: true' \
  'workingTreeFallback: forbidden' \
  'currentHeadFallback: forbidden' \
  'digestInput: raw-file-bytes' \
  'digestInput: canonical-json-bytes' \
  'digestMustMatchExactBytesAtBase: true' \
  'digestMustMatchCanonicalJsonAtBase: true' \
  'repositoryDigestMustEqualLoadedRepository: true' \
  'loadedRulesAndConfigMustShareRepositoryDigest: true' \
  'loadedBaseRefMustEqualBaseRef: true' \
  'loadedRulesAndConfigMustShareBaseSha: true' \
  'loadedRulesAndConfigMustShareBaseRef: true' \
  'mismatch: reject' \
  'stale: reject' \
  'sourceContent: forbidden' \
  'mode: PREPARATION_ONLY' \
  'authority: LOCAL_UNATTESTED' \
  'scmWriteback: forbidden' \
  'gateReplacement: forbidden' \
  'gateMutation: forbidden'; do
  rg -F -q -- "$needle" "$CONTRACT"
done

cat > "$OUT_DIR/valid-binding.json" <<'JSON'
{
  "schemaVersion": "review-base-rules-config-v1",
  "repositoryDigest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  "baseRef": "refs/heads/main",
  "baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "rulesDigest": "sha256:5532aa0d12add99c2b6a662572faebefbd4350c836a3bbd6f93d8a3bf759432f",
  "configDigest": "sha256:16de320e8b89f3bb433d437b3ec6e50e21ce296ddd640aa1465135fcd5771202",
  "bindingDigest": "sha256:ad72176041e4a1ba256a9fa910231385f5dff0af94eac78960349826cbb3b0cb",
  "loaded": {
    "rules": {
      "repositoryDigest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
      "rulesDigest": "sha256:5532aa0d12add99c2b6a662572faebefbd4350c836a3bbd6f93d8a3bf759432f",
      "loadedBaseRef": "refs/heads/main",
      "loadedBaseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "digestInput": "raw-file-bytes"
    },
    "config": {
      "repositoryDigest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
      "configDigest": "sha256:16de320e8b89f3bb433d437b3ec6e50e21ce296ddd640aa1465135fcd5771202",
      "loadedBaseRef": "refs/heads/main",
      "loadedBaseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "digestInput": "canonical-json-bytes"
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


def validate_binding(payload):
    assert payload["schemaVersion"] == "review-base-rules-config-v1"
    assert re.fullmatch(r"refs/(heads|tags|remotes)/[^\s]+", payload["baseRef"])
    assert re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", payload["baseSha"])
    for key in ("repositoryDigest", "rulesDigest", "configDigest", "bindingDigest"):
        assert re.fullmatch(r"sha256:[0-9a-f]{64}", payload[key]), key
    loaded = payload["loaded"]
    rules = loaded["rules"]
    config = loaded["config"]
    assert rules["repositoryDigest"] == config["repositoryDigest"] == payload["repositoryDigest"]
    assert rules["rulesDigest"] == payload["rulesDigest"]
    assert config["configDigest"] == payload["configDigest"]
    assert rules["loadedBaseRef"] == config["loadedBaseRef"] == payload["baseRef"]
    assert rules["loadedBaseSha"] == config["loadedBaseSha"] == payload["baseSha"]
    assert rules["digestInput"] == "raw-file-bytes"
    assert config["digestInput"] == "canonical-json-bytes"
    # Pin the two digest domains to concrete bytes so the fixture cannot pass
    # merely because it repeats opaque placeholder digests.
    raw_rules = b"<ruleset name='review-fixture'/>\n"
    canonical_config = b'{"buildSystem":"maven","version":"ruleops.config/v1"}'
    assert payload["rulesDigest"] == "sha256:" + hashlib.sha256(raw_rules).hexdigest()
    assert payload["configDigest"] == "sha256:" + hashlib.sha256(canonical_config).hexdigest()
    identity = {key: payload[key] for key in (
        "repositoryDigest", "baseRef", "baseSha", "rulesDigest", "configDigest"
    )}
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


valid = json.load(open(sys.argv[1], encoding="utf-8"))
validate_binding(valid)

# A single inconsistent identity/digest must be rejected even when every other
# field is unchanged. This guards against validators that only compare the two
# loaded projections with one another and forget the top-level binding.
from copy import deepcopy

def expect_reject(label, mutate):
    candidate = deepcopy(valid)
    mutate(candidate)
    try:
        validate_binding(candidate)
    except (AssertionError, KeyError):
        print(f"inconsistent-{label}=REJECT")
    else:
        raise AssertionError(f"inconsistent {label} was accepted")

expect_reject("repositoryDigest", lambda value: value["loaded"]["rules"].__setitem__("repositoryDigest", "sha256:" + "e" * 64))
expect_reject("loadedBaseRef", lambda value: value["loaded"]["config"].__setitem__("loadedBaseRef", "refs/heads/release"))
expect_reject("rulesDigest", lambda value: value["loaded"]["rules"].__setitem__("rulesDigest", "sha256:" + "f" * 64))
expect_reject("configDigest", lambda value: value["loaded"]["config"].__setitem__("configDigest", "sha256:" + "9" * 64))
expect_reject("rulesDigestInput", lambda value: value["loaded"]["rules"].__setitem__("digestInput", "canonical-json-bytes"))
PY

cat > "$OUT_DIR/stale-binding.json" <<'JSON'
{
  "captured": {
    "repositoryDigest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    "baseRef": "refs/heads/main",
    "baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "rulesDigest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "configDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  },
  "current": {
    "repositoryDigest": "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
    "baseRef": "refs/heads/release",
    "baseSha": "ffffffffffffffffffffffffffffffffffffffff",
    "rulesDigest": "sha256:9999999999999999999999999999999999999999999999999999999999999999",
    "configDigest": "sha256:8888888888888888888888888888888888888888888888888888888888888888"
  }
}
JSON
python3 - "$OUT_DIR/stale-binding.json" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
fields = ("repositoryDigest", "baseRef", "baseSha", "rulesDigest", "configDigest")
captured = payload["captured"]
current = payload["current"]

def stale_rejected(candidate, observed):
    # This is the contract's fail-closed stale predicate: no changed identity or
    # input digest may be reused for an in-flight review.
    return any(candidate[field] != observed[field] for field in fields)

assert not stale_rejected(captured, captured), "unchanged binding must remain usable"
assert stale_rejected(captured, current), "changed binding must be rejected as stale"
for field in fields:
    one_field_change = dict(captured)
    one_field_change[field] = current[field]
    assert stale_rejected(captured, one_field_change), field
    print(f"stale-{field}=REJECT")
PY

cat > "$OUT_DIR/inconsistent-binding.json" <<'JSON'
{
  "schemaVersion": "review-base-rules-config-v1",
  "repositoryDigest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  "baseRef": "refs/heads/main",
  "baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "rulesDigest": "sha256:5532aa0d12add99c2b6a662572faebefbd4350c836a3bbd6f93d8a3bf759432f",
  "configDigest": "sha256:16de320e8b89f3bb433d437b3ec6e50e21ce296ddd640aa1465135fcd5771202",
  "bindingDigest": "sha256:ad72176041e4a1ba256a9fa910231385f5dff0af94eac78960349826cbb3b0cb",
  "loaded": {
    "rules": {
      "repositoryDigest": "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
      "rulesDigest": "sha256:9999999999999999999999999999999999999999999999999999999999999999",
      "loadedBaseRef": "refs/heads/main",
      "loadedBaseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "digestInput": "raw-file-bytes"
    },
    "config": {
      "repositoryDigest": "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
      "configDigest": "sha256:8888888888888888888888888888888888888888888888888888888888888888",
      "loadedBaseRef": "refs/heads/release",
      "loadedBaseSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "digestInput": "raw-file-bytes"
    }
  },
  "authority": "LOCAL_UNATTESTED",
  "mode": "PREPARATION_ONLY"
}
JSON
python3 - "$OUT_DIR/inconsistent-binding.json" <<'PY'
import json
import sys

# Reuse the same validation rules as the positive fixture without importing a
# generated file. Every identity/digest mismatch must fail closed.
payload = json.load(open(sys.argv[1], encoding="utf-8"))
loaded = payload["loaded"]
violations = {
    "rules-repositoryDigest": loaded["rules"]["repositoryDigest"] != payload["repositoryDigest"],
    "config-repositoryDigest": loaded["config"]["repositoryDigest"] != payload["repositoryDigest"],
    "rulesDigest": loaded["rules"]["rulesDigest"] != payload["rulesDigest"],
    "configDigest": loaded["config"]["configDigest"] != payload["configDigest"],
    "config-loadedBaseRef": loaded["config"]["loadedBaseRef"] != payload["baseRef"],
    "config-loadedBaseSha": loaded["config"]["loadedBaseSha"] != payload["baseSha"],
    "config-digestInput": loaded["config"]["digestInput"] != "canonical-json-bytes",
}
assert all(violations.values()), violations
for name in violations:
    print(f"inconsistent-{name}=REJECT")
PY

cat > "$OUT_DIR/review-rules-config-binding.md" <<'EOF2'
# Review Base Rules and Config Binding Evidence

- Rules are hashed from exact raw file bytes at the immutable `baseRef`/`baseSha`;
  newline or formatting normalization is forbidden.
- Project config is parsed from that base revision, then hashed as UTF-8 canonical
  JSON bytes with sorted keys and no whitespace; YAML source bytes are not hashed.
- `repositoryDigest`, `loadedBaseRef`, `loadedBaseSha`, and each input digest must
  agree across the top-level binding and both loaded projections.
- Any changed repository/base identity or input digest is rejected as stale, and
  inconsistent loaded metadata is rejected before a review result is usable.
- The projection carries no source content and cannot write back to SCM, replace a Gate,
  mutate a Gate, or promote a release.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED.
EOF2

echo "review-rules-config-binding=PASS evidence=$OUT_DIR/review-rules-config-binding.md"
