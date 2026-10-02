#!/usr/bin/env python3
"""Reproducible offline check with existing validators/compiler and REAL PMD 7.9.

Usage: python3 scripts/standard-rule-extract-test.py SOURCE.html NEW_OUTPUT_DIRECTORY
Needs local JDK21/Maven and cached reactor dependencies; never downloads anything.
Each subprocess has a 60-second hard timeout. Logs and artifacts stay under OUTPUT.
"""
from copy import deepcopy
from html import unescape
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/standard-rule-extract.py"
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("standard_rule_extract", SCRIPT)
extract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(extract)


def main():
    source, out = (Path(value).resolve() for value in sys.argv[1:])
    out.mkdir(parents=True, exist_ok=False)
    raw = source.read_bytes()
    original_digest = extract.digest(raw)
    steps = []

    def run(label, command, expected=0):
        result = subprocess.run([str(value) for value in command], cwd=ROOT,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
        (out / (label + ".log")).write_bytes(result.stdout)
        steps.append({"check": label, "rc": result.returncode, "expectedRc": expected})
        extract_path = out / "steps.json"
        extract_path.write_text(json.dumps(steps, indent=2) + "\n")
        assert result.returncode == expected, f"{label}: rc={result.returncode}, see {out / (label + '.log')}"
        print(f"{label}: rc={result.returncode}", flush=True)
        return result.stdout.decode()

    inventory = extract.extract_document(raw)
    assert inventory == extract.extract_document(raw)
    assert inventory["census"] == {"entryCount": 142, "categories": {
        "both": 51, "bad_only": 10, "good_only": 13, "unlabeled_only": 12, "no_pre": 56}}
    assert inventory["sixDigitCensus"]["entryCount"] == 140
    assert len(re.findall(r"【J\d{6}】", raw.decode())) == 141
    drafts = [r for r in inventory["records"] if r["status"] == "draft"]
    assert len(drafts) == 1 and drafts[0]["sourceEntry"]["id"] == "J000086"
    assert sum(r["status"] == "needs_review" for r in inventory["records"]) == 141
    assert not any(inventory["trust"].values())
    entry = drafts[0]["sourceEntry"]
    # Independent text extraction verifies raw code is retained (including CRLF), not rewritten in place.
    section = re.search(r'<h3[^>]*>[^<]*【J000086】.*?(?=<h3)', raw.decode(), re.S)[0]
    source_blocks = [unescape(re.sub(r"<[^>]+>", "", block))
                     for block in re.findall(r"<pre\b[^>]*>(.*?)</pre>", section, re.S)]
    assert source_blocks == [entry["examples"]["good"][0]["code"], entry["examples"]["bad"][0]["code"]]
    for key in ("bad", "good"):
        code = entry["examples"][key][0]["code"]
        normalized = drafts[0]["assets"]["intent"]["spec"]["examples"][key][0]
        assert normalized == extract.main_body(code) and "java.math.BigDecimal" in normalized
        assert re.findall(r'"(?:\\.|[^"\\])*"|//[^\r\n]*', code) == re.findall(
            r'"(?:\\.|[^"\\])*"|//[^\r\n]*', normalized)
    assert drafts[0]["assets"]["intent"]["spec"]["examples"]["holdout"] == []
    assert drafts[0]["assets"]["intent"]["spec"]["examples"]["nearMiss"] == []
    assert extract.main_body("class Unsupported {}") is None
    assert extract.main_body(entry["examples"]["bad"][0]["code"].replace("main(", "other(")) is None

    tiny = '<h3>【J123456】Title</h3><p>mentions 【J123456】</p><pre>x &lt; y</pre>'
    tiny += '<h3>【J12345678】Extended</h3><p>反例:</p><pre><code><span>x</span>()</code></pre><h2>FAQ</h2><pre>ignored</pre>'
    parsed = extract.parse_document(tiny.encode())
    assert len(parsed) == 2 and parsed[0]["examples"]["unlabeled"][0]["code"] == "x < y"
    assert parsed[1]["examples"]["bad"][0]["code"] == "x()"
    for malformed in (b"", b"<h3>unclosed", (tiny + '<h3>【J123456】duplicate</h3>').encode()):
        try:
            extract.parse_document(malformed)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid document accepted")

    class OfflineProvider:
        identity = "offline-review-test/v1"
        configuration = {"mode": "unsupported"}

        def extract(self, entry):
            entry["id"] = "mutated-provider-copy"
            return None

    alternate = extract.extract_document(raw, OfflineProvider())
    assert alternate["source"]["provider"] == OfflineProvider.identity
    assert all(r["status"] == "needs_review" for r in alternate["records"])
    assert alternate["records"][0]["sourceEntry"]["id"] == "J000000"
    assert alternate["draftDigest"] != inventory["draftDigest"] and not any(alternate["trust"].values())
    class FailingProvider(OfflineProvider):
        def extract(self, entry):
            raise RuntimeError("offline provider failed")

    try:
        extract.extract_document(raw, FailingProvider())
    except RuntimeError as error:
        assert str(error) == "offline provider failed"
    else:
        raise AssertionError("provider failure was hidden")
    changed = deepcopy(entry)
    changed["title"] = "允许 BigDecimal(double)"
    assert extract.DeterministicStandardRuleExtractor().extract(changed) is None
    changed = deepcopy(entry)
    changed["examples"]["good"] = []
    assert extract.DeterministicStandardRuleExtractor().extract(changed) is None
    try:
        extract.draft_assets(entry, {"authoritative": True})
    except ValueError:
        pass
    else:
        raise AssertionError("provider trust/unknown fields accepted")

    run("extract", [sys.executable, SCRIPT, source, out / "draft"])
    run("extract-repeat", [sys.executable, SCRIPT, source, out / "repeat"])
    assert (out / "draft/inventory.json").read_bytes() == (out / "repeat/inventory.json").read_bytes()
    run("no-overwrite", [sys.executable, SCRIPT, source, out / "draft"], 2)
    assert (out / "draft/inventory.json").read_bytes() == (out / "repeat/inventory.json").read_bytes()
    unknown = out / "unmapped.html"
    unknown.write_text(tiny, encoding="utf-8")
    run("needs-review", [sys.executable, SCRIPT, unknown, out / "unmapped"], 1)

    cp_file = out / "classpath.txt"
    run("maven-offline", ["mvn", "-o", "-q", "-pl", "control-plane/verification", "-am", "package",
                          "dependency:build-classpath", "-Dmdep.outputFile=" + str(cp_file)])
    cp = ":".join(str(ROOT / "control-plane" / name / "target/classes")
                  for name in ("authoring", "factory", "verification")) + ":" + cp_file.read_text().strip()
    java = ["java", "-cp", cp]
    standard = out / "draft/J000086.standard.yaml"
    intent = out / "draft/J000086.intent.yaml"
    run("validate", java + ["io.ruleops.authoring.ValidateCmd", standard, intent])

    def compile_verify(label, intent_path, expected=0):
        output = run(label + "-compile", java + ["io.ruleops.factory.CandidateCmd", intent_path, out / (label + "-candidates")])
        candidate = Path(next(line.removeprefix("CANDIDATE ") for line in output.splitlines() if line.startswith("CANDIDATE ")))
        fixtures = sorted((candidate / "fixtures").rglob("*.java"))
        # LocalVerification tests hits; javac independently rejects syntactically invalid or unresolved fixtures.
        run(label + "-javac", ["javac", "-d", out / (label + "-classes"), *fixtures])
        run(label + "-verify", java + ["io.ruleops.verification.VerifyCmd", candidate, out / (label + "-evidence")], expected)
        evidence = json.loads((out / (label + "-evidence/evidence-bundle.json")).read_text())
        assert evidence["trustLevel"] == "LOCAL_UNATTESTED" and evidence["engineVersion"] == "7.9.0"
        assert evidence["verdict"] == ("PASS" if expected == 0 else "FAIL")
        return candidate, evidence

    candidate, evidence = compile_verify("document", intent)
    assert [(f["category"], f["actualHits"]) for f in evidence["fixtures"]] == [("bad", 1), ("good", 0)]
    # Additional developer-written cases prove type semantics, NOT independent holdout qualification.
    variant = json.loads(intent.read_text())
    variant["spec"]["examples"] = {
        "bad": ["double renamed = 0.125; new java.math.BigDecimal(renamed);",
                "float value = 0.1f; new java.math.BigDecimal(value);"],
        "good": ['new java.math.BigDecimal("0.1");', "new java.math.BigDecimal(1);",
                 "class BigDecimal { BigDecimal(double value) {} } new BigDecimal(0.1);"],
        "holdout": [], "nearMiss": []}
    semantic_path = out / "semantic-cases.yaml"
    extract.write_json(semantic_path, variant)
    compile_verify("semantic", semantic_path)
    swapped = json.loads(intent.read_text())
    examples = swapped["spec"]["examples"]
    examples["bad"], examples["good"] = examples["good"], examples["bad"]
    swapped_path = out / "swapped-polarity.yaml"
    extract.write_json(swapped_path, swapped)
    compile_verify("swapped", swapped_path, 1)
    invalid = deepcopy(variant)
    invalid["spec"]["standard"]["scenario"] = "missing-scenario"
    invalid_path = out / "invalid-reference.yaml"
    extract.write_json(invalid_path, invalid)
    run("invalid-reference", java + ["io.ruleops.authoring.ValidateCmd", standard, invalid_path], 1)
    invalid["spec"]["requiredCapabilities"] = ["java.dataflow"]
    gap_path = out / "capability-gap.yaml"
    extract.write_json(gap_path, invalid)
    run("capability-gap", java + ["io.ruleops.factory.CandidateCmd", gap_path, out / "must-not-compile"], 1)
    assert not (out / "must-not-compile").exists()
    assert extract.digest(source.read_bytes()) == original_digest
    extract.write_json(out / "summary.json", {"result": "PASS", "sourceSnapshot": original_digest,
                       "draftDigest": inventory["draftDigest"], "candidate": str(candidate),
                       "census": inventory["census"], "steps": steps, "trust": inventory["trust"]})
    print("standard-rule-extract=PASS; draft only, no independent verification or release authority")


if __name__ == "__main__":
    main()
