'use strict';
// 跨端一致性 harness：给定固定动作电池与一份真实 rf explain 输出文件，
// 打印规范化文本行。VS Code 端与 IntelliJ 端 harness 必须逐行相同，
// 由 scripts/ide-client-check.sh diff 校验（EXECUTABLE 证据）。
// 阶段 4 追加：统一命令面（同一 map params 机械构造 argv）与扫描结果语义 battery。

const fs = require('fs');
const core = require('../src/core.js');

function pathReason(p) {
  const r = core.validateRelativePath(p);
  return r.ok ? 'OK' : r.reason;
}

function viewReason(v) {
  const r = core.validateFindingView(v);
  return r.ok ? 'OK' : r.reason;
}

// Param 描述子：与 IntelliJ ThinClientCore.Param 记录一一对应。
function P(name, kind, flag, required, values, hexDigest, repeatable, identity) {
  return { name, kind, flag, required, values: values || [], hexDigest: !!hexDigest, repeatable: !!repeatable, identity: !!identity, routeSelector: false };
}

// 统一命令面 battery：两端用同一份 Param 列表 + 同一组值构造 argv，输出须逐字相同。
function mapBatteryLines() {
  const mapped = (template, params, values) => core.buildMappedArgs({ commandId: 'battery', commandTemplate: template, params }, values);
  return [
    'map-init=' + mapped('rf init --repo-root PATH [--non-interactive] [OPTIONS]', [
      P('repoRoot', 'path', '--repo-root', true), P('output', 'path', '--output', false),
      P('language', 'enum', '--language', false, ['java']), P('buildSystem', 'enum', '--build-system', false, ['maven', 'gradle']),
      P('nonInteractive', 'bool', '--non-interactive', false),
    ], { repoRoot: 'src', language: 'java', buildSystem: 'maven', nonInteractive: 'true' }).join('|'),
    'map-rule-generate=' + mapped('rf rule generate INTENT.yaml OUTPUT_DIR', [
      P('intent', 'file', null, true), P('outputDir', 'dir', null, true),
    ], { intent: 'src/i.yaml', outputDir: 'src/out' }).join('|'),
    'map-standard-validate=' + mapped('rf standard validate FILE...', [
      P('files', 'path', null, true, [], false, true),
    ], { files: 'src/a.yaml,src/b.yaml' }).join('|'),
    'map-registry-show=' + mapped('rf registry show rules|standards ID --version VERSION --expected-digest SHA256 (--server URL | --connection-profile FILE) [--timeout-ms N]', [
      P('resource', 'enum', null, true, ['rules', 'standards']), P('id', 'text', null, true),
      P('version', 'text', '--version', true), P('expectedDigest', 'digest', '--expected-digest', true),
    ], { resource: 'rules', id: 'corp.java.x', version: '1.0.0', expectedDigest: 'sha256:' + 'a'.repeat(64) }).join('|'),
    'map-decision-evaluate=' + mapped('rf decision evaluate INPUT_DIGEST SIGNAL NOW OUTPUT [OPTIONS]', [
      P('inputDigest', 'digest', null, true, [], true), P('signal', 'number', null, true), P('now', 'number', null, true),
      P('output', 'file', null, true), P('nearMiss', 'bool', '--near-miss', false),
    ], { inputDigest: 'b'.repeat(64), signal: '5', now: '100', output: 'src/o.json', nearMiss: 'true' }).join('|'),
    'map-pilot-feedback=' + mapped('rf pilot feedback OBSERVER_OPTIONS...', [
      P('journal', 'file', '--journal', true), P('measurementId', 'text', '--measurement-id', true),
      P('classification', 'enum', '--classification', true, ['valid', 'documentation', 'scope', 'false-positive']),
      P('actor', 'text', '--actor', true, [], false, false, true),
    ], { journal: 'src/j.jsonl', measurementId: 'm1', classification: 'scope', actor: 'alice' }).join('|'),
  ];
}

// 扫描结果语义 battery：同一份既有 ScanResult/ScanFailure，两端渲染须逐字相同。
function scanFixtureFail() {
  return {
    apiVersion: 'rulefactory.io/v1', kind: 'ScanResult',
    status: 'FAIL', gateDecision: 'FAIL', authority: 'LOCAL_UNATTESTED', trust: 'LOCAL_UNATTESTED',
    ruleId: 'corp.java.smoke', planned: 1, executed: 1, failed: 0,
    findingRecords: [{
      findingId: 'f1', ruleId: 'corp.java.smoke', fingerprint: 'fp1', findingDigest: 'DDG',
      message: 'system exit', location: { kind: 'SOURCE_SPAN', path: 'src/Main.java', line: 3, column: 1, identityDigest: 'IDG' },
    }],
    diagnostics: [{ code: 'FINDINGS_PRESENT', stage: 'gate', message: 'contains findings', retry: 'Never', nextStep: 'review' }],
    progress: [], resultDigest: 'RDG',
  };
}
function scanFixtureFailure() {
  return {
    apiVersion: 'rulefactory.io/v1', kind: 'ScanFailure', exitCode: 2,
    diagnostics: [{ code: 'CLI_INPUT_INVALID', stage: 'input', message: 'bad input', retry: 'FixInput', nextStep: 'fix' }],
    progress: [], diagnosticDigest: 'X',
  };
}
const oneLine = text => String(text).replace(/\n/g, '\\n');

const rfStdoutFile = process.argv[2];
const rfStdout = rfStdoutFile ? fs.readFileSync(rfStdoutFile, 'utf8') : '';
const parsed = core.parseExplainText(rfStdout);

const goodView = {
  apiVersion: 'rulefactory.io/v1',
  kind: 'FindingView',
  findingId: 'fid-1',
  ruleId: 'R1',
  fingerprint: 'fp-1',
  message: 'msg',
  location: { path: 'src/Main.java', line: 3, column: 1 },
};
const forbiddenView = Object.assign({}, goodView, { source: 'x' });
const unknownView = Object.assign({}, goodView, { extra: 'x' });

const submitOpts = { server: 'https://s', timeoutMs: 5000 };
const holdReceipt = core.renderReceiptGuards({ status: 'RECEIVED_ONLY' });
const violateReceipt = core.renderReceiptGuards({ status: 'RECEIVED_ONLY', scmPublished: true });

const lines = [
  'explain-args=' + core.buildExplainArgs('f.json', null).join('|'),
  'feedback-args=' + core.buildFeedbackDraftArgs('f.json',
    { classification: 'true-fp', reason: 'why', actor: 'alice' }).join('|'),
  'exception-args=' + core.buildExceptionRequestArgs('f.json',
    { scope: 'src/Main.java', reason: 'why', actor: 'alice' }).join('|'),
  'submit-args=' + core.buildSubmitArgs('feedback', 'req.json', submitOpts).join('|'),
  'status-args=' + core.buildSubmissionStatusArgs({ server: 'https://s', requestId: 'rid-1' }).join('|'),
  'exit0=' + core.classifyExitCode(0),
  'exit2=' + core.classifyExitCode(2),
  'exit3=' + core.classifyExitCode(3),
  'path-abs=' + pathReason('/etc/passwd'),
  'path-dotdot=' + pathReason('a/../b'),
  'path-good=' + pathReason('src/Main.java'),
  'view-good=' + viewReason(goodView),
  'view-forbidden=' + viewReason(forbiddenView),
  'view-unknown=' + viewReason(unknownView),
  'receipt-hold=' + (holdReceipt.guardsHold ? 'HOLD' : holdReceipt.violations.join(',')),
  'receipt-violate=' + (violateReceipt.guardsHold ? 'HOLD' : violateReceipt.violations.join(',')),
  'status-unknown=' + (core.statusLabel('WEIRD').includes('fail closed') ? 'failclosed' : 'leak'),
  'parse-finding_id=' + (parsed.finding_id || ''),
  'parse-rule_id=' + (parsed.rule_id || ''),
  'parse-authority_status=' + (parsed.authority_status || ''),
  'parse-authoritative=' + (parsed.authoritative || ''),
  ...mapBatteryLines(),
  'scan-classify-0=' + core.classifyScanExit(0),
  'scan-classify-2=' + core.classifyScanExit(2),
  'scan-classify-3=' + core.classifyScanExit(3),
  'scan-classify-4=' + core.classifyScanExit(4),
  'scan-render-fail=' + oneLine(core.renderScanReport(scanFixtureFail(), 0, '')),
  'scan-render-failure=' + oneLine(core.renderScanReport(scanFixtureFailure(), 2, '')),
];

process.stdout.write(lines.join('\n') + '\n');

const { cases, projection } = require('./scope-fixtures');
for (const c of cases) process.stdout.write('scope-case-' + c.name + '=' + projection(c) + '\n');
