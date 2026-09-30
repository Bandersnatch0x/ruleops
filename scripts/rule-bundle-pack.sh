#!/usr/bin/env bash
# Compile a deterministic multi-rule RuleBundle from governed RuleIntents.
# Usage: scripts/rule-bundle-pack.sh OUTPUT_DIR INTENT.yaml [INTENT.yaml ...]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ "$#" -lt 3 ]; then
  echo "usage: $0 OUTPUT_DIR INTENT.yaml [INTENT.yaml ...]" >&2
  exit 2
fi

out_dir="$1"
shift
case "$out_dir" in
  /*) ;;
  *) out_dir="$repo_root/$out_dir" ;;
esac
mkdir -p "$out_dir"
out_dir="$(cd "$out_dir" && pwd)"
bundle_dir="$out_dir/bundle"
if [ -e "$bundle_dir" ]; then
  echo "output bundle directory already exists: $bundle_dir" >&2
  exit 2
fi

work="$(mktemp -d "$out_dir/.work.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# Reuse one factory classpath for the whole bundle. This keeps multi-rule
# packaging from reintroducing the per-intent Maven bootstrap that Phase 4
# previously paid for.
factory_cp_file="$work/factory-classpath"
(cd "$repo_root" && mvn -q -pl control-plane/factory -am package -DskipTests \
  dependency:build-classpath -Dmdep.outputFile="$factory_cp_file" -q)
[ -s "$factory_cp_file" ] || { echo "factory classpath bootstrap is empty" >&2; exit 1; }
export ERF_FACTORY_CP_FILE="$factory_cp_file"

candidate_dirs=()
for intent in "$@"; do
  case "$intent" in
    /*) intent_path="$intent" ;;
    *) intent_path="$repo_root/$intent" ;;
  esac
  [ -f "$intent_path" ] || { echo "RuleIntent not found: $intent_path" >&2; exit 2; }
  compiled="$(cd "$repo_root" && scripts/candidate-compile.sh "$intent_path" "$work/candidates")"
  candidate="$(printf '%s\n' "$compiled" | sed -n 's/^CANDIDATE //p')"
  [ -n "$candidate" ] || { echo "candidate path missing: $intent_path" >&2; exit 1; }
  candidate_dirs+=("$candidate")
done

mkdir -p "$bundle_dir"
python3 - "$bundle_dir" "${candidate_dirs[@]}" <<'PY'
import copy
import hashlib
import json
import re
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

NS = "http://pmd.sourceforge.net/ruleset/2.0.0"
ET.register_namespace("", NS)
RULE_ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*\Z")

bundle_dir = Path(sys.argv[1])
candidate_paths = [Path(value) for value in sys.argv[2:]]
rules = []
seen_ids = set()
seen_engine_names = set()

for candidate in candidate_paths:
    manifest_path = candidate / "manifest.json"
    ruleset_path = candidate / "ruleset.xml"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    intent = manifest.get("intent") or {}
    rule_id = intent.get("id")
    version = intent.get("version")
    rule_name = manifest.get("ruleName")
    digest = manifest.get("digest")
    if not isinstance(rule_id, str) or not RULE_ID.fullmatch(rule_id):
        raise SystemExit(f"invalid candidate intent id: {manifest_path}")
    if rule_id in seen_ids:
        raise SystemExit(f"duplicate RuleIntent id: {rule_id}")
    if not isinstance(version, str) or not version or any(ord(char) < 32 for char in version):
        raise SystemExit(f"invalid candidate version: {rule_id}")
    if not isinstance(rule_name, str) or not rule_name or any(ord(char) < 32 for char in rule_name):
        raise SystemExit(f"invalid candidate engine rule name: {rule_id}")
    if rule_name in seen_engine_names:
        raise SystemExit(f"duplicate engine rule name: {rule_name}")
    if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise SystemExit(f"invalid candidate digest: {rule_id}")

    root = ET.fromstring(ruleset_path.read_bytes())
    if root.tag != f"{{{NS}}}ruleset":
        raise SystemExit(f"candidate ruleset has an unsupported root: {rule_id}")
    rule_nodes = [node for node in root if node.tag == f"{{{NS}}}rule"]
    if len(rule_nodes) != 1:
        raise SystemExit(f"candidate must contain exactly one PMD rule: {rule_id}")
    rule_node = copy.deepcopy(rule_nodes[0])
    if rule_node.get("name") != rule_name:
        raise SystemExit(f"candidate manifest/ruleset ruleName mismatch: {rule_id}")

    seen_ids.add(rule_id)
    seen_engine_names.add(rule_name)
    rules.append({
        "candidate": candidate,
        "id": rule_id,
        "version": version,
        "ruleName": rule_name,
        "template": manifest.get("template", ""),
        "candidateDigest": digest,
        "fixtureCount": manifest.get("fixtureCount", 0),
        "node": rule_node,
    })

if len(rules) < 2:
    raise SystemExit("a multi-rule RuleBundle requires at least two distinct candidates")

rules.sort(key=lambda item: item["id"])
identity_seed = "\n".join(f"{item['id']}@{item['version']}" for item in rules).encode("utf-8")
bundle_id = "bundle.java." + hashlib.sha256(identity_seed).hexdigest()[:16]

combined = ET.Element(f"{{{NS}}}ruleset", {"name": "erf-rule-bundle"})
description = ET.SubElement(combined, f"{{{NS}}}description")
description.text = "ERF multi-rule bundle: " + ", ".join(item["id"] for item in rules)
for item in rules:
    combined.append(item["node"])
ET.indent(combined, space="  ")
ruleset_bytes = (
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
    + ET.tostring(combined, encoding="unicode")
    + "\n"
).encode("utf-8")
(bundle_dir / "ruleset.xml").write_bytes(ruleset_bytes)

rule_metadata = [
    {
        "id": item["id"],
        "version": item["version"],
        "ruleName": item["ruleName"],
        "template": item["template"],
        "candidateDigest": item["candidateDigest"],
    }
    for item in rules
]
fixture_count = sum(int(item["fixtureCount"]) for item in rules)

# Runner canonical payload identity: manifest.json is deliberately excluded;
# the digest binds every executable bundle file in sorted path order.
payload_files = {"ruleset.xml": ruleset_bytes}
payload = bytearray()
for relative in sorted(payload_files):
    name_bytes = relative.encode("utf-8")
    file_bytes = payload_files[relative]
    payload.extend(struct.pack(">Q", len(name_bytes)))
    payload.extend(name_bytes)
    payload.extend(struct.pack(">Q", len(file_bytes)))
    payload.extend(file_bytes)
bundle_digest = hashlib.sha256(payload).hexdigest()

manifest = {
    "artifact": "ruleset.xml",
    "bundle": {"id": bundle_id, "version": "1.0.0"},
    "digest": bundle_digest,
    "fixtureCount": fixture_count,
    "intent": {"id": bundle_id, "version": "1.0.0"},
    "kind": "erf-rule-bundle",
    "ruleCount": len(rule_metadata),
    "rules": rule_metadata,
    "schemaVersion": "erf-rule-bundle-v1",
}
(bundle_dir / "manifest.json").write_text(
    json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)

print(f"BUNDLE_ID {bundle_id}")
print(f"BUNDLE_DIGEST {bundle_digest}")
print(f"RULE_COUNT {len(rule_metadata)}")
PY

bundle_id="$(jq -r '.bundle.id' "$bundle_dir/manifest.json")"
archive="$out_dir/ruleops-rulebundle-$bundle_id.tar.gz"
# Archive metadata is fixed so the same inputs yield the same release bytes.
tar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner \
  -C "$bundle_dir" -cf - . | gzip -n > "$archive"
archive_sha="$(sha256sum "$archive" | awk '{print $1}')"

printf 'BUNDLE_DIR %s\n' "$bundle_dir"
printf 'BUNDLE_ARCHIVE %s\n' "$archive"
printf 'BUNDLE_ID %s\n' "$bundle_id"
printf 'BUNDLE_DIGEST %s\n' "$(jq -r '.digest' "$bundle_dir/manifest.json")"
printf 'ARCHIVE_SHA256 %s\n' "$archive_sha"
