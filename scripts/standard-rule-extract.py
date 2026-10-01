#!/usr/bin/env python3
"""Offline HTML standard -> non-authoritative ERF drafts (JSON is valid YAML).

Usage: python3 scripts/standard-rule-extract.py SOURCE.html NEW_OUTPUT_DIRECTORY
No code is executed, no assets are published, and existing outputs are refused.
Exit: 0 at least one draft (NOT verified), 1 no mapping, 2 input/provider failure.
"""
import argparse
from collections import Counter
from copy import deepcopy
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import re
from typing import Protocol


def digest(value):
    data = value if isinstance(value, bytes) else json.dumps(
        value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()
    return "sha256:" + hashlib.sha256(data).hexdigest()


class StandardHtmlParser(HTMLParser):
    """Only heading markers start entries; never infer polarity from code order."""
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.entries = []
        self.entry = None
        self.tag = None
        self.parts = []
        self.line = 0
        self.polarity = "unlabeled"

    def handle_starttag(self, tag, attrs):
        if tag in ("h1", "h2", "h3", "h4", "h5", "h6", "p", "pre") and self.tag is None:
            self.tag, self.parts, self.line = tag, [], self.getpos()[0]

    def handle_data(self, data):
        if self.tag:
            self.parts.append(data)

    def handle_endtag(self, tag):
        if tag != self.tag:
            return
        raw = "".join(self.parts)
        text = raw.strip().removesuffix("↬").strip()
        self.tag = None
        if tag.startswith("h"):
            self.entry, self.polarity = None, "unlabeled"
            marker = re.search(r"【(J\d+)】(.+)", text)
            if marker:
                self.entry = {"id": marker[1], "title": marker[2], "line": self.line,
                              "text": [], "examples": {"bad": [], "good": [], "unlabeled": []}}
                self.entries.append(self.entry)
        elif self.entry is not None:
            if tag == "pre":
                self.entry["examples"][self.polarity].append({"line": self.line, "code": raw})
            else:
                self.entry["text"].append(text)
                label = re.match(r"^(正例|反例)\s*[:：]", text)
                if label:
                    self.polarity = "good" if label[1] == "正例" else "bad"
                elif text.startswith("检查工具"):
                    self.polarity = "unlabeled"


def parse_document(data):
    parser = StandardHtmlParser()
    parser.feed(data.decode("utf-8"))
    parser.close()
    ids = [entry["id"] for entry in parser.entries]
    if parser.tag is not None or not ids or len(set(ids)) != len(ids):
        raise ValueError("incomplete HTML, empty inventory or duplicate entry heading")
    return parser.entries


def census(entries):
    def category(entry):
        examples = entry["examples"]
        if examples["bad"] and examples["good"]:
            return "both"
        if examples["bad"]:
            return "bad_only"
        if examples["good"]:
            return "good_only"
        return "unlabeled_only" if examples["unlabeled"] else "no_pre"
    return {"entryCount": len(entries), "categories": dict(sorted(Counter(map(category, entries)).items()))}


class StandardRuleExtractor(Protocol):
    """Caller-injected OFFLINE provider; no dynamic loading or network adapter.

    identity includes provider/version; configuration contains no secrets.
    extract receives a copy of one source entry and returns None for needs_review,
    or {statement, target, requiredCapabilities, examples:{bad,good}, normalization}.
    Exceptions propagate as failures. Output is untrusted, not AuthoringSuggestion
    wire data (that contract deliberately excludes source code). A provider cannot
    grant trust, independent verification, coverage or publication authority.
    """
    identity: str
    configuration: dict

    def extract(self, entry: dict) -> dict | None: ...


def main_body(code):
    # ponytail: only the reviewed single-main shape; use a Java AST importer for other forms.
    match = re.fullmatch(
        r"\s*import java\.math\.BigDecimal;\s*public class \w+\s*\{\s*"
        r"public static void main\(String\[\] args\)\s*\{(.*)\n\s*}\s*}\s*", code, re.S)
    if not match or '"""' in code:
        return None
    # Qualify only type tokens, never alter strings/comments containing the name.
    tokens = r'//[^\r\n]*|/\*[\s\S]*?\*/|"(?:\\.|[^"\\])*"|\x27(?:\\.|[^\x27\\])*\x27|\bBigDecimal\b'
    return re.sub(tokens, lambda m: "java.math.BigDecimal" if m[0] == "BigDecimal" else m[0],
                  match[1]).strip()


class DeterministicStandardRuleExtractor:
    identity = "deterministic-standard-map/v1"
    configuration = {"mapping": "J000086", "engine": "pmd-7.9.0", "normalizer": "single-main-v1"}

    def extract(self, entry):
        if entry["id"] != "J000086" or entry["title"] != (
                "强制：为了防止精度损失，禁止使用构造方法 BigDecimal(double) 的方式把 double 值转化为 BigDecimal 对象"):
            return None
        examples = entry["examples"]
        if any(len(examples[key]) != 1 for key in ("bad", "good")) or examples["unlabeled"]:
            return None
        normalized = {key: [main_body(examples[key][0]["code"])] for key in ("bad", "good")}
        if any(not values[0] for values in normalized.values()):
            return None
        return {
            "statement": "禁止通过 BigDecimal(double) 构造方法把二进制浮点值转换为精确十进制对象",
            "target": {"kind": "existing-rule", "declaringType": "pmd", "invocation": "any",
                       "method": "category/java/errorprone.xml/AvoidDecimalLiteralsInBigDecimalConstructor",
                       "parameters": []},
            # ExistingRuleTemplate's current declared capability; PMD itself resolves the signature.
            "requiredCapabilities": ["java.ast"], "examples": normalized,
            "normalization": "single-main-v1: remove import/class/main wrappers; qualify BigDecimal type tokens; preserve body statements, literals and comments",
        }


def draft_assets(entry, proposal):
    if not isinstance(proposal, dict) or set(proposal) != {
            "statement", "target", "requiredCapabilities", "examples", "normalization"}:
        raise ValueError("invalid provider proposal fields")
    for key in ("bad", "good"):
        values = proposal["examples"][key]
        if not isinstance(values, list) or not values or any(not isinstance(v, str) or not v.strip() for v in values):
            raise ValueError("provider proposal requires nonempty bad/good code")
    standard_id, scenario = "JAVA-" + entry["id"], entry["id"].lower()
    standard = {"apiVersion": "rulefactory.io/v1", "kind": "Standard",
                "metadata": {"id": standard_id, "version": "0.1.0", "owner": "unassigned-draft"},
                "spec": {"title": proposal["statement"], "description": "Imported draft; ownership and coverage require review.",
                         "languages": ["java"], "scenarios": [{"id": scenario, "expected": "detectable"}]}}
    intent = {"apiVersion": "rulefactory.io/v1", "kind": "RuleIntent",
              "metadata": {"id": "draft.java." + scenario, "version": "0.1.0"}, "spec": {
                  "standard": {"id": standard_id, "version": "0.1.0", "scenario": scenario},
                  "language": {"id": "java", "sourceLevels": ["21"]},
                  "intent": {"statement": proposal["statement"]},
                  "scope": {"includeSourceSets": ["main"], "excludeSourceSets": ["test"]},
                  "semantics": {"target": proposal["target"]}, "requiredCapabilities": proposal["requiredCapabilities"],
                  "unresolved": {"behavior": "unknown"},
                  "examples": {"bad": proposal["examples"]["bad"], "good": proposal["examples"]["good"],
                               "holdout": [], "nearMiss": []}, "severity": {"default": "advisory"}}}
    return {"standard": standard, "intent": intent}


def extract_document(data, extractor: StandardRuleExtractor | None = None):
    extractor = extractor if extractor is not None else DeterministicStandardRuleExtractor()
    if not isinstance(extractor.identity, str) or not extractor.identity.strip():
        raise ValueError("provider identity is required")
    entries = parse_document(data)
    source = {"sourceSnapshot": digest(data), "configDigest": digest(extractor.configuration),
              "provider": extractor.identity, "extractorDigest": digest(Path(__file__).read_bytes())}
    records = []
    for entry in entries:
        proposal = extractor.extract(deepcopy(entry))
        record = {"sourceEntry": entry, "status": "needs_review", "reason": "unmapped_or_unsupported_example_shape"}
        if proposal is not None:
            record.update(status="draft", reason="requires_authoring_validation_and_independent_review",
                          normalization=proposal["normalization"], assets=draft_assets(entry, proposal))
        records.append(record)
    result = {"kind": "StandardRuleDraftInventory", "schemaVersion": 1, "source": source,
              "trust": {"authoritative": False, "independentlyReviewed": False, "usableForGate": False},
              "independentFixtures": "not_available: holdout/nearMiss are empty",
              "census": census(entries), "sixDigitCensus": census([e for e in entries if re.fullmatch(r"J\d{6}", e["id"])]),
              "records": records}
    result["draftDigest"] = digest(result)
    return result


def write_json(path, value):
    with path.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("source", type=Path)
    args.add_argument("output", type=Path)
    options = args.parse_args()
    try:
        result = extract_document(options.source.read_bytes())
        options.output.mkdir(parents=True, exist_ok=False)
        write_json(options.output / "inventory.json", result)
        count = 0
        for record in result["records"]:
            if record["status"] == "draft":
                for kind, asset in record["assets"].items():
                    write_json(options.output / (record["sourceEntry"]["id"] + "." + kind + ".yaml"), asset)
                count += 1
        print(json.dumps({"drafts": count, "needs_review": len(result["records"]) - count, "draftDigest": result["draftDigest"]}))
        return 0 if count else 1
    except (OSError, ValueError, KeyError, TypeError) as error:
        args.exit(2, "EXTRACTION_FAILED: " + str(error) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
