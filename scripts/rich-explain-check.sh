#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: rich-explain-check.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

CONTRACT="contracts/rich-explain-v1.yaml"
ADR="docs/adr/0164-rich-explain-and-authority-status-boundary.md"
EXPLAIN="runtime/thin-client/src/explain.rs"
CLI="runtime/thin-client/src/main.rs"
SIGN_EXAMPLE="runtime/thin-client/examples/rule_meta_sign.rs"
AUTHORITY_SIGN_EXAMPLE="runtime/thin-client/examples/authority_context_sign.rs"

binding="sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
other_binding="sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
rule_id="corp.java.rich-explain"
fingerprint="rich-explain-fp-1"

# Behavior: the rich explanation must come from verified, identity-matched
# metadata and must fail closed otherwise.
cargo test -q -p erf-thin-client explain::tests > "$out_dir/cargo-test.log" 2>&1
rg -q 'test result: ok\. [1-9][0-9]* passed; 0 failed; 0 ignored' "$out_dir/cargo-test.log"

# Static: contract, decision record and implementation carry the boundary.
rg -q 'rich-explain-v1|EXPLAIN_ONLY|DIGEST_ONLY' "$CONTRACT"
rg -q 'canProduceGateDecision: false|canChangeTrustState: false|canWriteScm: false|canUploadSource: false' "$CONTRACT"
rg -q 'METADATA_MISSING|METADATA_EXPIRED|METADATA_REVOKED|DIGEST_MISMATCH|RULE_IDENTITY_MISMATCH' "$CONTRACT"
rg -q 'TRUSTED_CI|LOCAL_UNATTESTED|INCONCLUSIVE' "$CONTRACT" "$ADR" "$EXPLAIN"
rg -q 'pub fn explain_finding|RefusalReason|RichExplanationRefusal|RULE_IDENTITY_MISMATCH' "$EXPLAIN"
rg -q 'fail-closed|失败关闭|权威' "$ADR"
rg -q 'rf explain FINDING.json' rf scripts/cli-surface-check.sh

cat > "$out_dir/finding.json" <<EOF
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "FindingView",
  "findingId": "rich-explain-finding-1",
  "ruleId": "$rule_id",
  "fingerprint": "$fingerprint",
  "message": "System.exit is forbidden in library code",
  "location": {"path": "src/main/java/Sample.java", "line": 7, "column": 3}
}
EOF

write_unsigned_metadata() {
  local path="$1" status="$2" meta_rule_id="$3"
  cat > "$path" <<EOF
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "RuleExplanationMetadata",
  "schemaVersion": "rule-explanation-metadata-v1",
  "ruleId": "$meta_rule_id",
  "ruleVersion": "2.3.0",
  "severity": "blocking",
  "status": "$status",
  "rationale": "System.exit terminates the JVM and bypasses orderly shutdown hooks.",
  "remediation": "Throw a checked exception and let the entrypoint decide how to exit.",
  "standardId": "SEC-JAVA-017",
  "standardVersion": "2.1.0",
  "bindingDigest": "$binding"
}
EOF
}

sign_metadata() {
  local unsigned="$1" signed="$2"
  cargo run -q -p erf-thin-client --example rule_meta_sign -- "$unsigned" "$signed"
}

write_result() {
  local path="$1" gate="$2" authority="$3" ctx_rule_id="$4" ctx_binding="$5" ctx_fp="$6"
  cat > "$path" <<EOF
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "AuthorityContext",
  "gateDecision": "$gate",
  "authority": "$authority",
  "ruleId": "$ctx_rule_id",
  "bindingDigest": "$ctx_binding",
  "findingFingerprint": "$ctx_fp"
}
EOF
}

sign_authority() {
  local unsigned="$1" signed="$2" trust="$3"
  cargo run -q -p erf-thin-client --example authority_context_sign -- "$unsigned" "$signed" "$trust"
}

cat > "$out_dir/assessment.json" <<'EOF'
{
  "apiVersion": "rulefactory.io/v1",
  "kind": "FindingAssessmentView",
  "baseline": "HISTORICAL",
  "exception": "NOT_APPLICABLE"
}
EOF

# Provisional explanation: no metadata, non-authoritative, exit 0.
./rf explain "$out_dir/finding.json" > "$out_dir/provisional.txt"
rg -q '^authoritative=false$' "$out_dir/provisional.txt"
rg -q '^authority_status=LOCAL_UNATTESTED$' "$out_dir/provisional.txt"
rg -q '^location=src/main/java/Sample\.java:7:3$' "$out_dir/provisional.txt"
rg -q '^next_step=' "$out_dir/provisional.txt"

# Authoritative explanation from active, identity-matched metadata + trusted result.
write_unsigned_metadata "$out_dir/meta-active.unsigned.json" active "$rule_id"
sign_metadata "$out_dir/meta-active.unsigned.json" "$out_dir/meta-active.json"
write_result "$out_dir/result-trusted.unsigned.json" FAIL TRUSTED_CI "$rule_id" "$binding" "$fingerprint"
sign_authority "$out_dir/result-trusted.unsigned.json" "$out_dir/result-trusted.json" "$out_dir/authority-trust.json"
./rf explain "$out_dir/finding.json" \
  --rule-meta "$out_dir/meta-active.json" \
  --result "$out_dir/result-trusted.json" \
  --authority-trust "$out_dir/authority-trust.json" \
  --assessment "$out_dir/assessment.json" \
  --format json > "$out_dir/authoritative.json"
rg -q '"authoritative": true' "$out_dir/authoritative.json"
rg -q '"authorityStatus": "TRUSTED_CI"' "$out_dir/authoritative.json"
rg -q '"severity": "blocking"' "$out_dir/authoritative.json"
rg -q '"standardId": "SEC-JAVA-017"' "$out_dir/authoritative.json"
rg -q '"baseline": "HISTORICAL"' "$out_dir/authoritative.json"
rg -q '"explanationDigest": "sha256:' "$out_dir/authoritative.json"

# Inconclusive gate collapses the authority label regardless of attestation.
write_result "$out_dir/result-inconclusive.unsigned.json" INCONCLUSIVE TRUSTED_CI "$rule_id" "$binding" "$fingerprint"
sign_authority "$out_dir/result-inconclusive.unsigned.json" "$out_dir/result-inconclusive.json" "$out_dir/authority-trust-inconclusive.json"
./rf explain "$out_dir/finding.json" \
  --rule-meta "$out_dir/meta-active.json" \
  --result "$out_dir/result-inconclusive.json" \
  --authority-trust "$out_dir/authority-trust-inconclusive.json" \
  --format json > "$out_dir/inconclusive.json"
rg -q '"authorityStatus": "INCONCLUSIVE"' "$out_dir/inconclusive.json"

assert_refusal() {
  local label="$1" reason="$2"
  shift 2
  set +e
  ./rf explain "$@" > "$out_dir/$label.txt" 2>&1
  local rc=$?
  set -e
  [ "$rc" -eq 3 ] || { echo "$label returned $rc, expected 3" >&2; cat "$out_dir/$label.txt" >&2; exit 1; }
  rg -q "^refused=true$" "$out_dir/$label.txt"
  rg -q "^reason=$reason$" "$out_dir/$label.txt"
  rg -q '^next_step=' "$out_dir/$label.txt"
}

# Expired metadata fails closed.
write_unsigned_metadata "$out_dir/meta-expired.unsigned.json" expired "$rule_id"
sign_metadata "$out_dir/meta-expired.unsigned.json" "$out_dir/meta-expired.json"
assert_refusal expired METADATA_EXPIRED "$out_dir/finding.json" --rule-meta "$out_dir/meta-expired.json"

# Revoked metadata fails closed.
write_unsigned_metadata "$out_dir/meta-revoked.unsigned.json" revoked "$rule_id"
sign_metadata "$out_dir/meta-revoked.unsigned.json" "$out_dir/meta-revoked.json"
assert_refusal revoked METADATA_REVOKED "$out_dir/finding.json" --rule-meta "$out_dir/meta-revoked.json"

# Metadata for a different rule fails closed.
write_unsigned_metadata "$out_dir/meta-other.unsigned.json" active "corp.java.other-rule"
sign_metadata "$out_dir/meta-other.unsigned.json" "$out_dir/meta-other.json"
assert_refusal identity RULE_IDENTITY_MISMATCH "$out_dir/finding.json" --rule-meta "$out_dir/meta-other.json"

# Result bound to a different binding digest fails closed.
write_result "$out_dir/result-wrong-binding.unsigned.json" PASS TRUSTED_CI "$rule_id" "$other_binding" "$fingerprint"
sign_authority "$out_dir/result-wrong-binding.unsigned.json" "$out_dir/result-wrong-binding.json" "$out_dir/authority-trust-wrong-binding.json"
assert_refusal binding DIGEST_MISMATCH "$out_dir/finding.json" \
  --rule-meta "$out_dir/meta-active.json" --result "$out_dir/result-wrong-binding.json" \
  --authority-trust "$out_dir/authority-trust-wrong-binding.json"

# Result bound to a different fingerprint fails closed.
write_result "$out_dir/result-wrong-fp.unsigned.json" PASS TRUSTED_CI "$rule_id" "$binding" "rich-explain-fp-2"
sign_authority "$out_dir/result-wrong-fp.unsigned.json" "$out_dir/result-wrong-fp.json" "$out_dir/authority-trust-wrong-fp.json"
assert_refusal fingerprint DIGEST_MISMATCH "$out_dir/finding.json" \
  --rule-meta "$out_dir/meta-active.json" --result "$out_dir/result-wrong-fp.json" \
  --authority-trust "$out_dir/authority-trust-wrong-fp.json"

# A trusted result without metadata cannot yield an authoritative explanation.
assert_refusal missing METADATA_MISSING "$out_dir/finding.json" --result "$out_dir/result-trusted.json" \
  --authority-trust "$out_dir/authority-trust.json"

input_identity="$(sha256sum "$CONTRACT" "$ADR" "$EXPLAIN" "$CLI" "$SIGN_EXAMPLE" "$AUTHORITY_SIGN_EXAMPLE" scripts/rich-explain-check.sh rf | sha256sum | awk '{print $1}')"
cat > "$out_dir/rich-explain.md" <<EOF
# Rich Explain Acceptance

- Acceptance command: scripts/rich-explain-check.sh $out_dir
- Contract: contracts/rich-explain-v1.yaml
- Decision: docs/adr/0164-rich-explain-and-authority-status-boundary.md
- Input identity: sha256:$input_identity
- Evidence level: EXECUTABLE
- Authority: EXPLAIN_ONLY; no Gate, Trust, Exception, retry, SCM or source upload

| Boundary | Evidence | Result |
| --- | --- | --- |
| Without metadata the explanation is provisional and LOCAL_UNATTESTED | provisional.txt | PASS |
| Active, identity-matched metadata + DSSE/TrustMetadata-verified result yields an authoritative TRUSTED_CI explanation | authoritative.json | PASS |
| An inconclusive gate collapses the authority label to INCONCLUSIVE | inconclusive.json | PASS |
| Expired metadata fails closed with a next step | expired.txt | PASS |
| Revoked metadata fails closed with a next step | revoked.txt | PASS |
| Metadata for another rule fails closed | identity.txt | PASS |
| A result bound to another binding digest fails closed | binding.txt | PASS |
| A result bound to another fingerprint fails closed | fingerprint.txt | PASS |
| A trusted result without metadata refuses as METADATA_MISSING | missing.txt | PASS |
| A self-declared TRUSTED_CI result without a verified proof is rejected | cargo explain::tests | PASS |
| Metadata is closed, digest-only and self-digest verified | cargo explain::tests | PASS |

The explanation only surfaces verified metadata bound to the finding identity.
Rule metadata, authority result and finding identity must agree; any expiry,
revocation, or digest/identity mismatch refuses with a typed reason and a next
step. This layer never emits a Gate decision, approves an exception, changes
trust state, retries, writes SCM, or uploads source.
EOF

echo "rich-explain=PASS evidence=$out_dir/rich-explain.md evidenceLevel=EXECUTABLE inputIdentity=sha256:$input_identity"
