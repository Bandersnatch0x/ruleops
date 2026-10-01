#!/usr/bin/env python3
"""ERF end-to-end suite runner.

Reads scripts/e2e/e2e-cases.json, executes the `run`-kind cases in fresh output
directories under the report dir, records rc/duration/evidence, and writes
report.json + report.md. `manual` cases are reported as MANUAL with their runbook.

Usage:
  run-e2e.sh --list
  run-e2e.sh --all [--out DIR]
  run-e2e.sh --case E2E-L7-003 [--case ...]
  run-e2e.sh --lifecycle L7-publish
  run-e2e.sh --form IDE
  run-e2e.sh --catalog docs/e2e/catalog.md
Placeholders in cmd/requires: {OUT} {REPO} {PMD_HOME} {RULESET} {STANDARD_SOURCE}
{CANDIDATE} {RUNTIME_HOME} {INSTALLED_RF}
"""
import argparse
import datetime
import json
import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
CASES = ROOT / "scripts/e2e/e2e-cases.json"

DEFAULTS = {
    "PMD_HOME": os.environ.get("PMD_HOME", "/mnt/d/Downloads/code-inspection/pmd-incremental-check/tools/pmd-bin-7.23.0"),
    "RULESET": os.environ.get("RULESET", "/mnt/d/Downloads/code-inspection/pmd-incremental-check/resources/pmd-ruleset-1.0.xml"),
    "STANDARD_SOURCE": os.environ.get("STANDARD_SOURCE", "/mnt/d/Downloads/java语言编码规范.txt"),
    "CANDIDATE": os.environ.get("CANDIDATE", str(ROOT / ".scratch/phase2-exit-check/candidates/corp.java.no-system-exit")),
    "RUNTIME_HOME": os.environ.get("RUNTIME_HOME", str(ROOT / ".scratch/production-runtime-pack-check/production-rf")),
    "INSTALLED_RF": os.environ.get("INSTALLED_RF", str(ROOT / ".scratch/production-runtime-pack-check/production-rf/rf")),
    "REPO": str(ROOT),
}


def load_cases():
    return json.loads(CASES.read_text(encoding="utf-8"))["cases"]


def subst(value, case_dir, run_dir):
    mapping = dict(DEFAULTS)
    mapping["OUT"] = str(case_dir)
    mapping["RUNOUT"] = str(run_dir)
    for key, rep in mapping.items():
        value = value.replace("{" + key + "}", rep)
    return value


def missing_requires(case, run_dir):
    return [r for r in case.get("requires", []) if not pathlib.Path(subst(r, run_dir, run_dir)).exists()]


def run_case(case, out_dir):
    case_dir = out_dir / case["id"]
    case_dir.mkdir(parents=True, exist_ok=True)
    cmd = [subst(c, case_dir, out_dir) for c in case["cmd"]]
    log = case_dir / "run.log"
    started = time.monotonic()
    status, rc = "PASS", None
    try:
        with log.open("w", encoding="utf-8", errors="replace") as fh:
            proc = subprocess.run(cmd, cwd=ROOT, stdout=fh, stderr=subprocess.STDOUT,
                                  timeout=case.get("timeout") or None)
        rc = proc.returncode
        if case.get("expectRc") is not None and rc != case["expectRc"]:
            status = "FAIL"
    except subprocess.TimeoutExpired:
        status, rc = "TIMEOUT", 124
    except OSError as error:
        status, rc = "ERROR", None
        log.write_text(f"OSError: {error}\n", encoding="utf-8")
    ev = case.get("evidence")
    evidence = str(case_dir) if ev in (None, "—", "") or str(ev).startswith("(") else str(case_dir / ev)
    return {
        "id": case["id"], "title": case["title"], "lifecycle": case["lifecycle"],
        "form": case["form"], "kind": case["kind"], "status": status, "rc": rc,
        "expectedRc": case.get("expectRc"), "durationMs": int((time.monotonic() - started) * 1000),
        "evidence": evidence, "log": str(log), "note": case.get("expectNote"),
    }


def select(cases, args):
    if args.case:
        wanted = set(args.case)
        return [c for c in cases if c["id"] in wanted]
    out = cases
    if args.lifecycle:
        out = [c for c in out if args.lifecycle.lower() in c["lifecycle"].lower()]
    if args.form:
        out = [c for c in out if args.form.lower() in c["form"].lower()]
    if args.kind:
        out = [c for c in out if c["kind"] == args.kind]
    return out


def write_report(results, out_dir):
    counts = {}
    for r in results:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
    report = {
        "kind": "E2EReport",
        "generatedAt": datetime.datetime.now().astimezone().isoformat(),
        "repoRoot": str(ROOT),
        "summary": {"total": len(results), **counts},
        "cases": results,
    }
    (out_dir / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    lines = ["# ERF E2E 运行报告", "",
             f"- 生成时间：{report['generatedAt']}",
             f"- 汇总：total={len(results)} " + " ".join(f"{k}={v}" for k, v in sorted(counts.items())), "",
             "| 用例 | 生命周期 | 形态 | 状态 | rc | 期望 | 证据 |", "|---|---|---|---|---:|---:|---|"]
    for r in results:
        lines.append(f"| {r['id']} | {r['lifecycle']} | {r['form']} | {r['status']} | "
                     f"{r['rc'] if r['rc'] is not None else '—'} | {r['expectedRc'] if r['expectedRc'] is not None else '—'} | "
                     f"{r['evidence'] or '—'} |")
    noted = [r for r in results if r.get("note") or r.get("reason") or r.get("runbook")]
    if noted:
        lines += ["", "## 说明与非通过项", ""]
        for r in noted:
            for key in ("note", "reason", "runbook"):
                if r.get(key):
                    lines.append(f"- `{r['id']}` ({r['status']}) {key}: {r[key]}")
    (out_dir / "report.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_catalog(path):
    cases = load_cases()
    lines = ["# ERF E2E 用例目录（自动生成，源：scripts/e2e/e2e-cases.json）", "",
             "状态列由 `scripts/e2e/run-e2e.sh` 生成；本目录仅描述用例。", "",
             "| 用例 ID | 标题 | 生命周期 | 形态 | 类型 | 期望 rc | 入口 | 证据 |", "|---|---|---|---|---|---:|---|---|"]
    for c in cases:
        lines.append(f"| {c['id']} | {c['title']} | {c['lifecycle']} | {c['form']} | {c['kind']} | "
                     f"{c['expectRc'] if c.get('expectRc') is not None else '—'} | `{c.get('entry','')}` | {c.get('evidence','—')} |")
    pathlib.Path(path).write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {path} ({len(cases)} cases)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--all", action="store_true")
    group.add_argument("--case", action="append")
    group.add_argument("--lifecycle")
    group.add_argument("--form")
    group.add_argument("--kind")
    group.add_argument("--list", action="store_true")
    group.add_argument("--catalog")
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    if args.list:
        for c in load_cases():
            print(f"{c['id']:16s} {c['kind']:6s} {c['lifecycle']:14s} {c['form']:22s} {c['title']}")
        return 0
    if args.catalog:
        write_catalog(args.catalog)
        return 0

    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    out_dir = pathlib.Path(args.out) if args.out else ROOT / f".scratch/e2e/{stamp}"
    out_dir.mkdir(parents=True, exist_ok=True)

    results = []
    for case in select(load_cases(), args):
        if case["kind"] != "run":
            results.append({"id": case["id"], "title": case["title"], "lifecycle": case["lifecycle"],
                            "form": case["form"], "kind": "manual", "status": "MANUAL", "rc": None,
                            "expectedRc": None, "durationMs": 0, "evidence": None, "log": None,
                            "runbook": case.get("runbook")})
            print(f"MANUAL {case['id']} {case['title']}")
            continue
        miss = missing_requires(case, out_dir)
        if miss:
            results.append({"id": case["id"], "title": case["title"], "lifecycle": case["lifecycle"],
                            "form": case["form"], "kind": "run", "status": "SKIP", "rc": None,
                            "expectedRc": case.get("expectRc"), "durationMs": 0, "evidence": None,
                            "log": None, "reason": f"missing requires: {miss}"})
            print(f"SKIP   {case['id']} missing {miss}")
            continue
        result = run_case(case, out_dir)
        results.append(result)
        print(f"{result['status']:6s} {case['id']} rc={result['rc']} ({result['durationMs']}ms) {case['title']}")

    write_report(results, out_dir)
    counts = {}
    for r in results:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
    print(f"\nreport: {out_dir}/report.md  summary: " + " ".join(f"{k}={v}" for k, v in sorted(counts.items())))
    return 0 if counts.get("FAIL", 0) == 0 and counts.get("ERROR", 0) == 0 and counts.get("TIMEOUT", 0) == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
