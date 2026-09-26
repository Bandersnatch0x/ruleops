#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
[ "$#" -eq 1 ] || { echo "usage: delivery-cli-vertical-check.sh <production-pack-output-dir>" >&2; exit 2; }
out_dir="$1"
package_dir="$out_dir/production-rf"
[ -x "$package_dir/rf" ] || {
    echo "production package missing; run production-runtime-pack-check.sh first" >&2
    exit 2
}
runtime_trust_root_pem="$(<"$out_dir/runtime-signing-public.pem")"
[ -n "$runtime_trust_root_pem" ] || {
    echo "production runtime trust root missing; run production-runtime-pack-check.sh first" >&2
    exit 2
}
python3 - "$out_dir/delivery-trust-root-key.der" <<'PY'
import pathlib
import sys

# Deterministic acceptance-only root seed; production installs an independent
# offline root and injects its public key through the deployment boundary.
pathlib.Path(sys.argv[1]).write_bytes(
    bytes.fromhex("302e020100300506032b657004220420") + bytes([100]) * 32
)
PY
openssl pkey -inform DER -in "$out_dir/delivery-trust-root-key.der" \
    -out "$out_dir/delivery-trust-root-key.pem" >/dev/null 2>&1
openssl pkey -in "$out_dir/delivery-trust-root-key.pem" -pubout \
    -out "$out_dir/delivery-trust-root.pem" >/dev/null 2>&1
delivery_trust_root_hex="$(openssl pkey -pubin -in "$out_dir/delivery-trust-root.pem" -pubout -outform DER 2>/dev/null | tail -c 32 | od -An -tx1 -v | tr -d ' \n')"
[ "${#delivery_trust_root_hex}" -eq 64 ] || {
    echo "delivery trust root public key could not be derived" >&2
    exit 2
}

delivery_dir="$out_dir/delivery-vertical"
repo_dir="$delivery_dir/repo"
package_dir_abs="$(cd "$package_dir" && pwd)"
mkdir -p "$repo_dir/src/main/java/example" "$delivery_dir/candidates"
printf '%s\n' '<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>io.ruleops.acceptance</groupId><artifactId>delivery-vertical</artifactId><version>1.0</version></project>' > "$repo_dir/pom.xml"
printf '%s\n' 'package example; public final class Bad { void run() { System.exit(1); } }' > "$repo_dir/src/main/java/example/Bad.java"
python3 - "$delivery_dir/ruleset.xml" <<'PY'
import pathlib
import sys

pathlib.Path(sys.argv[1]).write_text(
    """<ruleset name="delivery-vertical"
 xmlns="http://pmd.sourceforge.net/ruleset/2.0.0"
 xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
 xsi:schemaLocation="http://pmd.sourceforge.net/ruleset/2.0.0 https://pmd.github.io/schema/ruleset_2_0_0.xsd">
 <description>signed delivery vertical</description>
 <rule name="NoSystemExit"
       language="java"
       message="forbidden call to java.lang.System.exit"
       class="net.sourceforge.pmd.lang.rule.xpath.XPathRule">
   <properties>
     <property name="xpath">
       <value><![CDATA[//MethodCall[pmd-java:matchesSig('java.lang.System#exit(int)')]]]></value>
     </property>
   </properties>
 </rule>
</ruleset>
""",
    encoding="utf-8",
)
PY

cargo run --quiet -p erf-project-config -- init \
    --repo-root "$repo_dir" \
    --force \
    --non-interactive \
    > "$delivery_dir/config-init.txt"

for profile in connected enterpriseAirGapped standaloneDeveloper; do
    mode=connected
    [ "$profile" = enterpriseAirGapped ] && mode=offline
    [ "$profile" = standaloneDeveloper ] && mode=standalone
    cargo run --quiet -p rf-cli --example make_delivery_fixture -- \
        "$repo_dir" \
        "$delivery_dir/ruleset.xml" \
        "$package_dir/lib/erf-java-host-0.1.0-shaded.jar" \
        "$package_dir/bin/rf-workspace-resolver" \
        "$repo_dir/.ruleops/delivery-$mode.json" \
        "$repo_dir/.ruleops/delivery-execution.lock" \
        "$profile" \
        pmd \
        "$package_dir/bin/rf-cli" \
        > "$delivery_dir/make-$mode.txt"
done

for mode in connected offline standalone; do
    command_args=(
        scan
        --repo-root "$repo_dir"
        --delivery-descriptor "$repo_dir/.ruleops/delivery-$mode.json"
        --format json
        --progress none
    )
    result_file="$repo_dir/.ruleops/scan-$mode.json"
    command_args+=(--output "$result_file")
    if [ "$mode" != connected ]; then
        command_args+=(--execution-mode "$mode")
    fi
    if RULEOPS_RUNTIME_RELEASE_TRUST_ROOT_PEM="$runtime_trust_root_pem" \
        RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX="$delivery_trust_root_hex" \
        "$package_dir/rf" "${command_args[@]}" > "$delivery_dir/$mode.stdout" 2> "$delivery_dir/$mode.stderr"; then
        mode_rc=0
    else
        mode_rc=$?
    fi
    [ "$mode_rc" -eq 0 ] || {
        echo "$mode delivery scan failed with exit $mode_rc" >&2
        exit 1
    }
    python3 - "$result_file" "$delivery_dir/$mode.stderr" "$mode" "$repo_dir" <<'PY'
import json
import pathlib
import sys

result = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
stderr = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")
mode = sys.argv[3]
repo = sys.argv[4]
assert result["kind"] == "ScanResult"
assert result["bundleDigest"].startswith("sha256:")
assert result["bundleDeliveryDescriptorDigest"].startswith("sha256:")
assert result["bundleCompatibilityIdentity"] == "java21-pmd7-spotbugs4.9.8"
assert result["bundlePlatform"] == "linux/amd64"
assert result["findingViews"], mode
serialized = json.dumps(result)
for forbidden in ("sourceContent", "sourceSnippet", "fullFinding", "credentials", "127.0.0.1", repo):
    assert forbidden not in serialized, (mode, forbidden)
assert "127.0.0.1" not in stderr
assert repo not in stderr
PY
done

python3 - "$repo_dir/.ruleops/scan-connected.json" "$repo_dir/.ruleops/scan-offline.json" "$repo_dir/.ruleops/scan-standalone.json" <<'PY'
import json
import pathlib
import sys

reports = [json.loads(pathlib.Path(path).read_text(encoding="utf-8")) for path in sys.argv[1:]]
digests = {report["semanticResultDigest"] for report in reports}
assert len(digests) == 1, digests
PY

# Repeat the complete delivery path with the bytecode backend. The resolver
# switches to SAFE_STATIC class-output discovery only for this locked backend;
# each profile still consumes its own signed descriptor and cache entry.
spotbugs_dir="$out_dir/delivery-vertical-spotbugs"
spotbugs_repo="$spotbugs_dir/repo"
mkdir -p "$spotbugs_repo/src/main/java/example" "$spotbugs_repo/target/classes"
printf '%s\n' '<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>io.ruleops.acceptance</groupId><artifactId>delivery-spotbugs</artifactId><version>1.0</version></project>' > "$spotbugs_repo/pom.xml"
printf '%s\n' 'package example; public final class Bad { void run() { System.exit(1); } }' > "$spotbugs_repo/src/main/java/example/Bad.java"
javac -g -d "$spotbugs_repo/target/classes" "$spotbugs_repo/src/main/java/example/Bad.java"
RULEOPS_RUNTIME_HOME="$package_dir" "$package_dir/rf" init \
    --repo-root "$spotbugs_repo" \
    --force \
    --non-interactive \
    > "$spotbugs_dir/config-init.txt"
for profile in connected enterpriseAirGapped standaloneDeveloper; do
    mode=connected
    [ "$profile" = enterpriseAirGapped ] && mode=offline
    [ "$profile" = standaloneDeveloper ] && mode=standalone
    cargo run --quiet -p rf-cli --example make_delivery_fixture -- \
        "$spotbugs_repo" \
        "$delivery_dir/ruleset.xml" \
        "$package_dir/lib/erf-java-host-0.1.0-shaded.jar" \
        "$package_dir/bin/rf-workspace-resolver" \
        "$spotbugs_repo/.ruleops/delivery-$mode.json" \
        "$spotbugs_repo/.ruleops/delivery-execution.lock" \
        "$profile" \
        spotbugs \
        "$package_dir/bin/rf-cli" \
        > "$spotbugs_dir/make-$mode.txt"
done

[ "$(sed -n 's/^backend_version=//p' "$spotbugs_repo/.ruleops/delivery-execution.lock")" = "4.9.8" ] || {
    echo "SpotBugs delivery lock did not freeze engine version 4.9.8" >&2
    exit 1
}
for mode in connected offline standalone; do
    spotbugs_result="$spotbugs_repo/.ruleops/scan-$mode.json"
    spotbugs_args=(
        scan
        --repo-root "$spotbugs_repo"
        --delivery-descriptor "$spotbugs_repo/.ruleops/delivery-$mode.json"
        --source-root target/classes
        --include 'target/classes/**'
        --exclude '.ruleops/**'
        --format json
        --progress none
        --output "$spotbugs_result"
    )
    [ "$mode" != connected ] && spotbugs_args+=(--execution-mode "$mode")
    RULEOPS_RUNTIME_RELEASE_TRUST_ROOT_PEM="$runtime_trust_root_pem" \
        RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX="$delivery_trust_root_hex" \
        "$package_dir/rf" "${spotbugs_args[@]}" > "$spotbugs_dir/$mode.stdout" 2> "$spotbugs_dir/$mode.stderr"
    python3 - "$spotbugs_result" "$mode" <<'PY'
import json
import pathlib
import sys

result = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert result["kind"] == "ScanResult"
assert result["bundleDigest"].startswith("sha256:")
assert result["bundleDeliveryDescriptorDigest"].startswith("sha256:")
assert result["findingRecords"], sys.argv[2]
PY
done

python3 - "$spotbugs_repo/.ruleops/scan-connected.json" "$spotbugs_repo/.ruleops/scan-offline.json" "$spotbugs_repo/.ruleops/scan-standalone.json" <<'PY'
import json
import pathlib
import sys

reports = [json.loads(pathlib.Path(path).read_text(encoding="utf-8")) for path in sys.argv[1:]]
digests = {report["semanticResultDigest"] for report in reports}
assert len(digests) == 1, digests
PY

# The delivery modes above exercise the production PMD path. Keep the second
# Java backend honest as well: drive the same shaded Host through its real
# framed protocol with compiled bytecode and require a SpotBugs observation.
ERF_JAVA_HOST_JAR="$package_dir_abs/lib/erf-java-host-0.1.0-shaded.jar" \
    cargo test --quiet -p erf-host-client --test java_host \
    java_host_negotiates_and_executes_spotbugs_bytecode

RULEOPS_DELIVERY_DESCRIPTOR="$repo_dir/.ruleops/delivery-connected.json" \
RULEOPS_RUNTIME_RELEASE_TRUST_ROOT_PEM="$runtime_trust_root_pem" \
RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX="$delivery_trust_root_hex" \
    "$package_dir/rf" scan \
    --repo-root "$repo_dir" \
    --format json \
    --progress none \
    --output "$repo_dir/.ruleops/scan-env.json" \
    > "$delivery_dir/env.stdout" 2> "$delivery_dir/env.stderr"
python3 - "$repo_dir/.ruleops/scan-env.json" <<'PY'
import json
import sys

result = json.load(open(sys.argv[1], encoding="utf-8"))
assert result["kind"] == "ScanResult"
assert result["bundleDigest"].startswith("sha256:")
assert result["bundleDeliveryDescriptorDigest"].startswith("sha256:")
PY

python3 - "$repo_dir/.ruleops/delivery-connected.json" "$repo_dir/.ruleops" <<'PY'
import json
import pathlib
import sys

base = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
root = pathlib.Path(sys.argv[2])

def write(name, mutate):
    value = json.loads(json.dumps(base))
    mutate(value)
    path = root / name
    path.write_text(json.dumps(value), encoding="utf-8")
    return path

write("invalid-unknown.json", lambda value: value.update({"futureField": True}))
write("invalid-platform.json", lambda value: value.update({"expectedPlatform": "linux/arm64"}))
write("trust-failure.json", lambda value: value["verification"]["evidence"].update({"signatureValid": False}))
write("stale-cache.json", lambda value: value["descriptor"].update({"expiresAtEpochSecs": 2}))
write("offline-cache-miss.json", lambda value: value.update({"profile": "enterpriseAirGapped", "cacheRoot": ".ruleops/missing-cache"}))
write("connected-fetch-failure.json", lambda value: value.update({"cacheRoot": ".ruleops/missing-cache"}))
def tamper_oci(value):
    value["verification"]["packs"][0]["ociLayout"]["indexBytesHex"] = "7b7d"
write("invalid-oci-layout.json", tamper_oci)
PY

run_failure() {
    local name="$1"
    local expected="$2"
    shift 2
    if RULEOPS_RUNTIME_RELEASE_TRUST_ROOT_PEM="$runtime_trust_root_pem" \
        RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX="$delivery_trust_root_hex" \
        "$package_dir/rf" scan --repo-root "$repo_dir" --delivery-descriptor "$repo_dir/.ruleops/$name.json" --format json --progress none "$@" > "$delivery_dir/failure-$name.json" 2> "$delivery_dir/failure-$name.stderr"; then
        rc=0
    else
        rc=$?
    fi
    [ "$rc" -eq "$expected" ] || {
        echo "$name expected exit $expected, got $rc" >&2
        exit 1
    }
    python3 - "$delivery_dir/failure-$name.json" "$delivery_dir/failure-$name.stderr" "$repo_dir" <<'PY'
import json
import pathlib
import sys

failure = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
stderr = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")
assert failure["kind"] == "ScanFailure"
assert failure["diagnostics"][0]["detailDigest"].startswith("sha256:")
assert "detail" not in failure["diagnostics"][0]
assert sys.argv[3] not in json.dumps(failure)
assert "127.0.0.1" not in json.dumps(failure)
assert stderr == ""
PY
}

run_failure invalid-unknown 2
run_failure invalid-platform 2
run_failure trust-failure 4
run_failure stale-cache 3
run_failure offline-cache-miss 3 --execution-mode offline
run_failure connected-fetch-failure 3
run_failure invalid-oci-layout 2

openssl genpkey -algorithm ED25519 -out "$delivery_dir/wrong-runtime-root-key.pem" >/dev/null 2>&1
openssl pkey -in "$delivery_dir/wrong-runtime-root-key.pem" -pubout -out "$delivery_dir/wrong-runtime-root.pem" >/dev/null 2>&1
if RULEOPS_RUNTIME_RELEASE_TRUST_ROOT_PEM="$(<"$delivery_dir/wrong-runtime-root.pem")" \
    RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX="$delivery_trust_root_hex" \
    "$package_dir/rf" scan --repo-root "$repo_dir" \
    --delivery-descriptor "$repo_dir/.ruleops/delivery-connected.json" \
    --format json --progress none > "$delivery_dir/failure-runtime-root.json" 2> "$delivery_dir/failure-runtime-root.stderr"; then
    wrong_root_rc=0
else
    wrong_root_rc=$?
fi
[ "$wrong_root_rc" -eq 4 ] || {
    echo "wrong runtime trust root expected exit 4, got $wrong_root_rc" >&2
    exit 1
}

openssl genpkey -algorithm ED25519 -out "$delivery_dir/wrong-delivery-root-key.pem" >/dev/null 2>&1
openssl pkey -in "$delivery_dir/wrong-delivery-root-key.pem" -pubout -out "$delivery_dir/wrong-delivery-root.pem" >/dev/null 2>&1
wrong_delivery_root_hex="$(openssl pkey -pubin -in "$delivery_dir/wrong-delivery-root.pem" -pubout -outform DER 2>/dev/null | tail -c 32 | od -An -tx1 -v | tr -d ' \n')"
if RULEOPS_RUNTIME_RELEASE_TRUST_ROOT_PEM="$runtime_trust_root_pem" \
    RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX="$wrong_delivery_root_hex" \
    "$package_dir/rf" scan --repo-root "$repo_dir" \
    --delivery-descriptor "$repo_dir/.ruleops/delivery-connected.json" \
    --format json --progress none > "$delivery_dir/failure-delivery-root.json" 2> "$delivery_dir/failure-delivery-root.stderr"; then
    wrong_delivery_root_rc=0
else
    wrong_delivery_root_rc=$?
fi
[ "$wrong_delivery_root_rc" -eq 4 ] || {
    echo "wrong delivery trust root expected exit 4, got $wrong_delivery_root_rc" >&2
    exit 1
}

python3 - "$out_dir/delivery-cli-vertical.md" "$delivery_dir" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
root = pathlib.Path(sys.argv[2])
path.write_text(
    "# rf delivery CLI vertical evidence\n\n"
    f"- output: {root}\n"
    "- runtime: real Java PMD/SpotBugs Host and SAFE_STATIC Resolver\n"
    "- signed input: RuleBundle, delegated TrustMetadata, two Runtime Pack statements, signed ExecutionLock\n"
    "- trust roots: delivery signer delegation and runtime-manifest Ed25519 DSSE each use an external deployment root; wrong roots rejected\n"
    "- profiles: Connected cache hit, Offline cache hit, Standalone cache hit\n"
    "- backend coverage: PMD delivery scan plus real SpotBugs bytecode Host observation\n"
    "- SpotBugs identity: signed ExecutionLock and negotiated Host capability are 4.9.8\n"
    "- negative paths: invalid descriptor, compatibility mismatch, trust failure, stale cache, offline miss, connected fetch failure\n"
    "- result binding: bundleDigest, descriptorDigest, compatibility identity and platform are present\n"
    "- privacy: structured failure detail is digest-only; endpoint and repository paths are absent\n"
    "- status: PASS\n",
    encoding="utf-8",
)
PY

echo "delivery-cli-vertical=PASS evidence=$out_dir/delivery-cli-vertical.md"
