'use strict';
// 跨端一致性 harness：给定固定动作电池与一份真实 rf explain 输出文件，
// 打印规范化文本行。VS Code 端与 IntelliJ 端 harness 必须逐行相同，
// 由 scripts/ide-client-check.sh diff 校验（EXECUTABLE 证据）。

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
    { scope: 'file', reason: 'why', actor: 'alice' }).join('|'),
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
];

process.stdout.write(lines.join('\n') + '\n');
