'use strict';
// 阶段 4：扫描结果工作台语义测试。
// 权威来自 runtime/scan-output 契约：ScanResult 的 status/gateDecision/authority
// 决定展示，绝不把 scan exit=0 当 PASS；ScanFailure 的 exitCode 采用 CLI 等级
// （2 输入无效 / 3 未决或拒绝 / 4 信任失败）；长任务进度不 5 秒截断。离线零依赖。

const test = require('node:test');
const assert = require('node:assert/strict');
const core = require('../src/core');

function scanResult(overrides = {}) {
  return {
    apiVersion: 'rulefactory.io/v1',
    kind: 'ScanResult',
    schemaVersion: 'scan-output-v1',
    status: 'FAIL',
    gateDecision: 'FAIL',
    authority: 'LOCAL_UNATTESTED',
    trust: 'LOCAL_UNATTESTED',
    ruleId: 'corp.java.smoke',
    planned: 1,
    executed: 1,
    failed: 0,
    completeness: { planned: 1, accounted: 1, failed: 0, missing: 0, observationErrors: 0, complete: true },
    findingRecords: [{
      findingId: 'f1', ruleId: 'corp.java.smoke', fingerprint: 'fp1', findingDigest: 'DDG',
      message: 'system exit', location: { kind: 'SOURCE_SPAN', path: 'src/Main.java', line: 3, column: 1, identityDigest: 'IDG' },
    }],
    diagnostics: [{ code: 'FINDINGS_PRESENT', stage: 'gate', message: 'contains findings', retry: 'Never', nextStep: 'review' }],
    progress: [{ stage: 'resolve', outcome: 'COMPLETED', elapsedNanos: 0, inputDigest: 'idg' }],
    resultDigest: 'RDG',
    ...overrides,
  };
}

test('scan exit=0 with FAIL status is displayed as FAIL, never PASS', () => {
  const parsed = core.parseScanStdout(JSON.stringify(scanResult()));
  assert.equal(parsed.ok, true);
  assert.equal(parsed.status, 'FAIL');
  const display = core.renderScanReport(parsed, 0, '');
  assert.doesNotMatch(display, /PASS（Gate 通过）/);
  assert.match(display, /FAIL（存在未豁免 finding/);
  assert.match(display, /不做成 PASS/);
});

test('LOCAL_UNATTESTED is not rendered as a production PASS even with exit 0', () => {
  const report = scanResult({ status: 'LOCAL_UNATTESTED', gateDecision: 'PASS', authority: 'LOCAL_UNATTESTED' });
  const parsed = core.parseScanStdout(JSON.stringify(report));
  const display = core.renderScanReport(parsed, 0, '');
  assert.match(display, /LOCAL_UNATTESTED/);
  assert.doesNotMatch(display, /^scan PASS/);
});

test('TRUSTED PASS with exit 0 renders as PASS', () => {
  const report = scanResult({ status: 'PASS', gateDecision: 'PASS', authority: 'TRUSTED', trust: 'TRUSTED' });
  const display = core.renderScanReport(core.parseScanStdout(JSON.stringify(report)), 0, '');
  assert.match(display, /PASS（Gate 通过）/);
});

test('INCONCLUSIVE scan with exit 0 still renders INCONCLUSIVE (long task outcome)', () => {
  const report = scanResult({ status: 'INCONCLUSIVE', gateDecision: 'INCONCLUSIVE' });
  const parsed = core.parseScanStdout(JSON.stringify(report));
  assert.equal(parsed.ok, true);
  const display = core.renderScanReport(parsed, 0, '');
  assert.match(display, /INCONCLUSIVE/);
  assert.doesNotMatch(display, /PASS（Gate 通过）/);
});

test('ScanFailure carries the CLI exit-class semantics (2/3/4)', () => {
  const failure = {
    apiVersion: 'rulefactory.io/v1', kind: 'ScanFailure', schemaVersion: 'scan-output-v1',
    exitCode: 2,
    diagnostics: [{ code: 'CLI_INPUT_INVALID', stage: 'input', message: 'bad input', retry: 'FixInput', nextStep: 'fix' }],
    progress: [], diagnosticDigest: 'X',
  };
  const parsed = core.parseScanStdout(JSON.stringify(failure));
  assert.equal(parsed.ok, true);
  assert.equal(parsed.kind, 'ScanFailure');
  assert.equal(parsed.exitCode, 2);
  assert.equal(core.classifyScanExit(2), 'INVALID_INPUT');
  assert.equal(core.classifyScanExit(3), 'INCONCLUSIVE_OR_REFUSED');
  assert.equal(core.classifyScanExit(4), 'TRUST_FAILED');
  const display = core.renderScanReport(parsed, 2, '');
  assert.match(display, /ScanFailure/);
  assert.match(display, /CLI_INPUT_INVALID/);
});

test('unparseable output fails closed as not-pass', () => {
  const parsed = core.parseScanStdout('not json at all');
  assert.equal(parsed.ok, false);
  const display = core.renderScanReport(parsed, 0, '');
  assert.doesNotMatch(display, /PASS（Gate 通过）/);
});

test('stderr progress lines surface long-task stages without a 5s truncation claim', () => {
  const stderr = ['progress stage=resolve outcome=COMPLETED elapsed_nanos=123', 'progress stage=gate outcome=FAILED elapsed_nanos=456', ''].join('\n');
  const lines = core.parseProgressLines(stderr);
  assert.equal(lines.length, 2);
  assert.match(lines[0], /stage=resolve/);
  const display = core.renderScanReport(core.parseScanStdout(JSON.stringify(scanResult())), 0, stderr);
  assert.match(display, /progress stage=gate outcome=FAILED/);
});

test('findings and diagnostics render with navigable location + heavy-context digests only', () => {
  const display = core.renderScanReport(core.parseScanStdout(JSON.stringify(scanResult())), 0, '');
  assert.match(display, /finding id=f1 rule=corp\.java\.smoke location=src\/Main\.java:3:1/);
  assert.match(display, /diagnostic code=FINDINGS_PRESENT/);
});