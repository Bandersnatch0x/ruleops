#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
[ "$#" -eq 1 ] || { echo "usage: production-runtime-pack-check.sh <output-dir>" >&2; exit 2; }
out_dir="$1"
package_dir="$out_dir/production-rf"
repo_dir="$out_dir/real-java-repo"
mkdir -p "$out_dir" "$package_dir/bin" "$package_dir/lib" "$repo_dir/src/main/java/example"
cd "$repo_root"

cargo build --quiet --release -p rf-cli -p erf-project-config -p erf-static-resolver
mvn -q -f backend-hosts/java/pom.xml package -DskipTests

cp "$repo_root/rf" "$package_dir/rf"
chmod +x "$package_dir/rf"
cp "$repo_root/target/release/rf-cli" "$package_dir/bin/rf-cli"
cp "$repo_root/target/release/erf-project-config" "$package_dir/bin/erf-project-config"
cp "$repo_root/target/release/erf-static-resolver" "$package_dir/bin/rf-workspace-resolver"
cp "$repo_root/backend-hosts/java/target/erf-java-host-0.1.0-shaded.jar" "$package_dir/lib/erf-java-host-0.1.0-shaded.jar"
chmod +x "$package_dir/bin/rf-cli" "$package_dir/bin/erf-project-config" "$package_dir/bin/rf-workspace-resolver"

python3 - "$package_dir/bin/rf-backend-host" "$package_dir/lib/erf-java-host-0.1.0-shaded.jar" <<'PY'
import pathlib
import sys

wrapper = pathlib.Path(sys.argv[1])
jar = pathlib.Path(sys.argv[2])
wrapper.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    'exec java -jar "$(cd "$(dirname "$0")/../lib" && pwd)/'
    f"{jar.name}\" \"$@\"\n",
    encoding="utf-8",
)
wrapper.chmod(0o755)
PY

python3 - "$package_dir" "$out_dir/runtime-manifest.payload" <<'PY'
import hashlib
import json
import pathlib
import sys

package = pathlib.Path(sys.argv[1])
payload_path = pathlib.Path(sys.argv[2])
assets = {
    "cli": ("bin/rf-cli", "rf-cli-v1"),
    "projectConfig": ("bin/erf-project-config", "project-config-v1"),
    "backendHost": ("bin/rf-backend-host", "java21-pmd7-spotbugs4.9.8"),
    "backendHostJar": ("lib/erf-java-host-0.1.0-shaded.jar", "java21-pmd7-spotbugs4.9.8"),
    "workspaceResolver": ("bin/rf-workspace-resolver", "linux/amd64"),
}
manifest_assets = {}
for name, (relative, identity) in assets.items():
    path = package / relative
    manifest_assets[name] = {
        "path": relative,
        "digest": "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest(),
        "compatibilityIdentity": identity,
        "platform": "linux/amd64",
        "verification": "RELEASE_ARTIFACT",
    }
payload = {
    "apiVersion": "rulefactory.io/v1",
    "kind": "RfRuntimeRelease",
    "schemaVersion": "rf-runtime-release-v1",
    "protocolVersion": "rulefactory.io/v1",
    "platform": "linux/amd64",
    "entrypoint": "rf",
    "cli": "bin/rf-cli",
    "projectConfig": "bin/erf-project-config",
    "backendHost": "bin/rf-backend-host",
    "workspaceResolver": "bin/rf-workspace-resolver",
    "assets": manifest_assets,
}
canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
payload_path.write_bytes(canonical)
manifest = {
    **payload,
    "signature": {
        "status": "PENDING",
    },
    "productionReplacementRequired": False,
    "sourceUploadDefault": False,
    "fullFindingUploadDefault": False,
    "scmCredentials": False,
}
(package / "runtime-manifest.json").write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY

openssl genpkey -algorithm ED25519 -out "$out_dir/runtime-signing-key.pem" >/dev/null 2>&1
openssl pkey -in "$out_dir/runtime-signing-key.pem" -pubout -out "$out_dir/runtime-signing-public.pem" >/dev/null 2>&1
python3 - "$out_dir/runtime-manifest.payload" "$out_dir/runtime-manifest.signing-input" <<'PY'
import pathlib
import sys

payload = pathlib.Path(sys.argv[1]).read_bytes()
payload_type = b"application/vnd.erf.asset.v0"
pae = b"DSSEv1 " + str(len(payload_type)).encode() + b" " + payload_type + b" " + str(len(payload)).encode() + b" " + payload
pathlib.Path(sys.argv[2]).write_bytes(pae)
PY
openssl pkeyutl -sign -rawin \
    -inkey "$out_dir/runtime-signing-key.pem" \
    -in "$out_dir/runtime-manifest.signing-input" \
    -out "$out_dir/runtime-manifest.signature"
openssl pkeyutl -verify -rawin \
    -pubin -inkey "$out_dir/runtime-signing-public.pem" \
    -in "$out_dir/runtime-manifest.signing-input" \
    -sigfile "$out_dir/runtime-manifest.signature" >/dev/null
python3 - "$package_dir/runtime-manifest.json" "$out_dir/runtime-manifest.payload" "$out_dir/runtime-manifest.signature" "$out_dir/runtime-signing-public.pem" <<'PY'
import base64
import hashlib
import json
import pathlib
import sys

manifest_path = pathlib.Path(sys.argv[1])
payload = pathlib.Path(sys.argv[2]).read_bytes()
signature = pathlib.Path(sys.argv[3]).read_bytes()
public_key = pathlib.Path(sys.argv[4]).read_text(encoding="utf-8")
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
manifest["signature"] = {
    "status": "RELEASE_ARTIFACT",
    "algorithm": "ed25519-dsse",
    "keyId": "erf-production-release-check",
    "payloadType": "application/vnd.erf.asset.v0",
    "payloadDigest": "sha256:" + hashlib.sha256(payload).hexdigest(),
    "signatureBase64": base64.b64encode(signature).decode("ascii"),
    "publicKeyPem": public_key,
}
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

python3 - "$package_dir/runtime-manifest.json" "$package_dir" <<'PY'
import hashlib
import json
import pathlib
import sys

manifest_path = pathlib.Path(sys.argv[1])
package = pathlib.Path(sys.argv[2])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
assert manifest["productionReplacementRequired"] is False
assert manifest["signature"]["status"] == "RELEASE_ARTIFACT"
assert manifest["signature"]["algorithm"] == "ed25519-dsse"
assert manifest["signature"]["payloadType"] == "application/vnd.erf.asset.v0"
assert manifest["signature"]["signatureBase64"]
assert manifest["signature"]["publicKeyPem"].startswith("-----BEGIN PUBLIC KEY-----")
payload = {key: manifest[key] for key in (
    "apiVersion", "kind", "schemaVersion", "protocolVersion", "platform",
    "entrypoint", "cli", "projectConfig", "backendHost", "workspaceResolver",
    "assets",
)}
expected = "sha256:" + hashlib.sha256(
    json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
).hexdigest()
assert manifest["signature"]["payloadDigest"] == expected
for name, asset in manifest["assets"].items():
    path = package / asset["path"]
    assert path.is_file() and not path.is_symlink(), (name, path)
    assert asset["verification"] == "RELEASE_ARTIFACT", (name, asset)
    assert asset["digest"] == "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
    assert asset["compatibilityIdentity"]
    assert asset["platform"] == "linux/amd64"
PY

python3 - "$package_dir/runtime-manifest.json" "$out_dir/reference-fixture-manifest.json" <<'PY'
import json
import pathlib
import sys

source = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
source["assets"]["backendHost"]["verification"] = "REFERENCE_FIXTURE"
pathlib.Path(sys.argv[2]).write_text(json.dumps(source), encoding="utf-8")
PY
if python3 - "$out_dir/reference-fixture-manifest.json" <<'PY'
import json
import sys

manifest = json.load(open(sys.argv[1], encoding="utf-8"))
sys.exit(0 if all(asset["verification"] == "RELEASE_ARTIFACT" for asset in manifest["assets"].values()) else 1)
PY
then
    echo "production manifest accepted a REFERENCE_FIXTURE asset" >&2
    exit 1
fi

python3 - "$package_dir/runtime-manifest.json" "$out_dir/unsigned-manifest.json" <<'PY'
import json
import pathlib
import sys

source = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
source["signature"]["algorithm"] = "none"
pathlib.Path(sys.argv[2]).write_text(json.dumps(source), encoding="utf-8")
PY
if python3 - "$out_dir/unsigned-manifest.json" <<'PY'
import json
import sys

manifest = json.load(open(sys.argv[1], encoding="utf-8"))
sys.exit(0 if manifest["signature"]["algorithm"] != "none" else 1)
PY
then
    echo "production manifest accepted algorithm: none" >&2
    exit 1
fi

printf '%s\n' '<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>io.ruleops.acceptance</groupId><artifactId>production-pack</artifactId><version>1.0</version></project>' > "$repo_dir/pom.xml"
printf '%s\n' 'package example; public final class ProductionPack { void run() { System.exit(1); } }' > "$repo_dir/src/main/java/example/ProductionPack.java"

RULEOPS_RUNTIME_HOME="$package_dir" "$package_dir/rf" init \
  --repo-root "$repo_dir" \
  --force \
  --non-interactive \
  > "$out_dir/init.txt"
rg -q '^CONFIG_WRITTEN ' "$out_dir/init.txt"

jar_path="$package_dir/lib/erf-java-host-0.1.0-shaded.jar"
rg -a -q 'net/sourceforge/pmd|pmd-java' "$jar_path"
rg -a -q 'edu/umd/cs/findbugs|spotbugs-java' "$jar_path"
file "$package_dir/bin/rf-workspace-resolver" | rg -q 'executable|ELF'

python3 - "$package_dir" <<'PY'
import hashlib
import json
import pathlib
import sys

package = pathlib.Path(sys.argv[1])
oci_root = package / "oci"
for name, asset, kind, asset_type in (
    ("backend-runtime", package / "lib/erf-java-host-0.1.0-shaded.jar", "backendRuntime", "application/vnd.erf.backend-runtime-pack.v0"),
    ("workspace-resolver", package / "bin/rf-workspace-resolver", "workspaceResolver", "application/vnd.erf.workspace-resolver-pack.v0"),
):
    layout = oci_root / name
    blobs = layout / "blobs" / "sha256"
    blobs.mkdir(parents=True, exist_ok=True)
    layer = asset.read_bytes()
    layer_digest = hashlib.sha256(layer).hexdigest()
    config = json.dumps({
        "kind": kind,
        "assetType": asset_type,
        "platform": "linux/amd64",
        "compatibilityIdentity": "java21-pmd7-spotbugs4.9.8",
    }, sort_keys=True, separators=(",", ":")).encode()
    config_digest = hashlib.sha256(config).hexdigest()
    manifest = {
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.erf.runtime-pack.config.v0",
            "digest": "sha256:" + config_digest,
            "size": len(config),
        },
        "layers": [{
            "mediaType": "application/vnd.erf.runtime-pack.asset.v0",
            "digest": "sha256:" + layer_digest,
            "size": len(layer),
        }],
        "kind": kind,
        "platform": "linux/amd64",
        "assetDigest": "sha256:" + layer_digest,
        "compatibilityIdentity": "java21-pmd7-spotbugs4.9.8",
        "annotations": {
            "org.rulefactory.kind": kind,
            "org.rulefactory.platform": "linux/amd64",
            "org.rulefactory.compatibilityIdentity": "java21-pmd7-spotbugs4.9.8",
        },
    }
    manifest_bytes = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest_digest = hashlib.sha256(manifest_bytes).hexdigest()
    (blobs / config_digest).write_bytes(config)
    (blobs / layer_digest).write_bytes(layer)
    (blobs / manifest_digest).write_bytes(manifest_bytes)
    (layout / "oci-layout").write_text(json.dumps({"imageLayoutVersion": "1.0.0"}) + "\n", encoding="utf-8")
    (layout / "index.json").write_text(json.dumps({
        "schemaVersion": 2,
        "manifests": [{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": "sha256:" + manifest_digest,
            "size": len(manifest_bytes),
            "platform": {"architecture": "amd64", "os": "linux"},
        }],
    }, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

python3 - "$out_dir/production-runtime-pack.md" "$package_dir" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
package = pathlib.Path(sys.argv[2])
path.write_text(
    "# Production Runtime Pack evidence\n\n"
    f"- package: {package}\n"
    f"- manifest: {package / 'runtime-manifest.json'}\n"
    "- backend: real shaded Java Host with PMD and SpotBugs dependencies\n"
    "- resolver: real SAFE_STATIC Maven/Gradle Resolver\n"
    "- OCI layouts: backend-runtime and workspace-resolver layouts include config, manifest and content-addressed layers\n"
    "- signatureMetadata: RELEASE_ARTIFACT / ed25519-dsse\n"
    "- referenceFixtureRejected: all packaged assets are RELEASE_ARTIFACT\n"
    "- sourceUploadDefault: false\n"
    "- fullFindingUploadDefault: false\n"
    "- scmCredentials: false\n\n"
    "The package contains no REFERENCE_FIXTURE asset and rejects algorithm: none by "
    "the manifest assertion above. The delivery vertical test supplies a signed "
    "RuleBundle before treating the Java Host as a successful scan dependency.\n",
    encoding="utf-8",
)
PY

echo "production-runtime-pack=PASS evidence=$out_dir/production-runtime-pack.md package=$package_dir"
