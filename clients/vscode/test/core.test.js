'use strict';
// 离线、零依赖核测试：node --test 直接执行 core.js 的真实行为。
const test = require('node:test');
const assert = require('node:assert/strict');
const core = require('../src/core');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { resolveSafeWorkspaceFile } = require('../src/path-safety');

const goodView = {
  apiVersion: 'rulefactory.io/v1',
  kind: 'FindingView',
  findingId: 'f-1',
  ruleId: 'rule-1',
  fingerprint: 'fp-1',
  message: 'example',
  location: { path: 'src/app/Main.java', line: 12, column: 3 },
};

test('validateRelativePath accepts a workspace-relative path', () => {
  assert.equal(core.validateRelativePath('src/app/Main.java').ok, true);
});

test('validateRelativePath rejects traversal and absolute/backslash/colon/control', () => {
  assert.equal(core.validateRelativePath('../secret').ok, false);
  assert.equal(core.validateRelativePath('/etc/passwd').ok, false);
  assert.equal(core.validateRelativePath('C:/win').ok, false);
  assert.equal(core.validateRelativePath('a\\b').ok, false);
  assert.equal(core.validateRelativePath('a:b').ok, false);
  assert.equal(core.validateRelativePath('a\u0000b').ok, false);
  assert.equal(core.validateRelativePath('./here').ok, false);
});

test('resolveSafeWorkspaceFile rejects symlinked directories and files', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'erf-vscode-path-'));
  const outside = fs.mkdtempSync(path.join(os.tmpdir(), 'erf-vscode-outside-'));
  try {
    fs.writeFileSync(path.join(root, 'inside.java'), 'class Inside {}');
    fs.writeFileSync(path.join(outside, 'outside.java'), 'class Outside {}');
    assert.equal(resolveSafeWorkspaceFile(root, 'inside.java'), path.join(root, 'inside.java'));
    fs.symlinkSync(path.join(outside, 'outside.java'), path.join(root, 'file-link.java'));
    assert.throws(() => resolveSafeWorkspaceFile(root, 'file-link.java'), /SYMLINK_COMPONENT/);
    fs.symlinkSync(outside, path.join(root, 'dir-link'));
    assert.throws(() => resolveSafeWorkspaceFile(root, 'dir-link/outside.java'), /SYMLINK_COMPONENT/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
    fs.rmSync(outside, { recursive: true, force: true });
  }
});

test('validateFindingView accepts a well-formed digest-only view', () => {
  assert.equal(core.validateFindingView(goodView).ok, true);
});

test('validateFindingView rejects forbidden source-bearing fields (privacy fail-closed)', () => {
  const withSource = Object.assign({}, goodView, { source: 'secret code' });
  const r = core.validateFindingView(withSource);
  assert.equal(r.ok, false);
  assert.match(r.reason, /FORBIDDEN_FIELD:source/);
});

test('validateFindingView rejects unknown fields (closed-world)', () => {
  const extra = Object.assign({}, goodView, { extra: 1 });
  assert.equal(core.validateFindingView(extra).ok, false);
});

test('validateFindingView rejects out-of-workspace location path', () => {
  const bad = Object.assign({}, goodView, { location: { path: '../x', line: 1, column: 1 } });
  const r = core.validateFindingView(bad);
  assert.equal(r.ok, false);
  assert.match(r.reason, /LOCATION_PATH/);
});

test('buildExplainArgs uses space-separated subcommand and json by default', () => {
  assert.deepEqual(core.buildExplainArgs('f.json', {}), ['explain', 'f.json', '--format', 'json']);
});

test('buildFeedbackDraftArgs constructs the feedback draft invocation', () => {
  const args = core.buildFeedbackDraftArgs('f.json',
    { classification: 'not-fp', reason: 'why', actor: 'dev' });
  assert.deepEqual(args,
    ['feedback', 'draft', 'f.json', '--classification', 'not-fp', '--reason', 'why', '--actor', 'dev']);
});

test('buildFeedbackDraftArgs fails closed on missing required args', () => {
  assert.throws(() => core.buildFeedbackDraftArgs('f.json', { classification: 'not-fp' }), /MISSING_ARG/);
  assert.throws(() => core.buildFeedbackDraftArgs('f.json', { classification: 'false_positive', reason: 'why', actor: 'dev' }), /BAD_CLASSIFICATION/);
});

test('buildExceptionRequestArgs constructs the exception request invocation', () => {
  const args = core.buildExceptionRequestArgs('f.json',
    { scope: 'src/app/**', reason: 'why', actor: 'dev' });
  assert.deepEqual(args,
    ['exception', 'request', 'f.json', '--scope', 'src/app/**', '--reason', 'why', '--actor', 'dev']);
});

test('buildSubmitArgs requires an explicit server endpoint (no auto-discovery)', () => {
  assert.throws(() => core.buildSubmitArgs('feedback', 'r.json', {}), /MISSING_ARG:serverEndpoint/);
  assert.deepEqual(core.buildSubmitArgs('feedback', 'r.json', { server: 'https://h:1' }),
    ['feedback', 'submit', 'r.json', '--server', 'https://h:1']);
});

test('buildSubmissionStatusArgs requires request-id or artifact-digest', () => {
  assert.throws(() => core.buildSubmissionStatusArgs({ server: 'https://h:1' }), /MISSING_ARG/);
  assert.deepEqual(
    core.buildSubmissionStatusArgs({ server: 'https://h:1', requestId: 'sha256:aa' }),
    ['submission', 'status', '--server', 'https://h:1', '--request-id', 'sha256:aa']);
});

test('classifyExitCode maps rf exit codes', () => {
  assert.equal(core.classifyExitCode(0), 'OK');
  assert.equal(core.classifyExitCode(2), 'INVALID_INPUT');
  assert.equal(core.classifyExitCode(3), 'REFUSAL_OR_PENDING');
});

test('parseExplainText parses key=value lines including location', () => {
  const parsed = core.parseExplainText('rule_id=rule-1\nlocation=src/app/Main.java:12:3\nauthority_status=TRUSTED_CI\n');
  assert.equal(parsed.rule_id, 'rule-1');
  assert.equal(parsed.location, 'src/app/Main.java:12:3');
  assert.equal(parsed.authority_status, 'TRUSTED_CI');
});

test('renderReceiptGuards holds when false-authority guards are all false', () => {
  const r = core.renderReceiptGuards(
    { status: 'RECEIVED_ONLY', exceptionApproved: false, gateMutated: false, scmPublished: false });
  assert.equal(r.guardsHold, true);
  assert.match(r.status, /RECEIVED_ONLY/);
});

test('renderReceiptGuards fails closed if any guard claims authority', () => {
  const r = core.renderReceiptGuards(
    { status: 'RECEIVED_ONLY', exceptionApproved: true, gateMutated: false, scmPublished: false });
  assert.equal(r.guardsHold, false);
  assert.deepEqual(r.violations, ['exceptionApproved']);
});

test('statusLabel fails closed on unknown status', () => {
  assert.match(core.statusLabel('WHATEVER'), /fail closed/);
});


const { cases: scopeCases, projection } = require('./scope-fixtures');
for (const c of scopeCases) test('shared scope fixture: ' + c.name, () => assert.equal(projection(c), c.expected));

test('scope choices reject stale selections and require explicit expansion confirmation', () => {
  const c = scopeCases[0]; const options = core.projectScopeOptions(c.finding, c.config, c.facts);
  assert.equal(core.selectScope(options, c.finding.location.path), c.finding.location.path);
  assert.throws(() => core.selectScope(options, 'app/**'), /CONFIRMATION/);
  assert.equal(core.selectScope(options, 'app/**', true), 'app/**');
  assert.throws(() => core.selectScope(options, '**', true), /STALE_SCOPE/);
  assert.throws(() => core.selectScope([], c.finding.location.path), /STALE_SCOPE/);
  assert.deepEqual(core.scopeCandidates({ ...c.finding, source: 'private' }, c.config), []);
});
test('explain authority trust remains paired with result and argv-safe', () => {
  assert.deepEqual(core.buildExplainArgs('finding.json', { result: 'result with spaces.json', authorityTrust: 'trust.json' }),
    ['explain', 'finding.json', '--format', 'json', '--result', 'result with spaces.json', '--authority-trust', 'trust.json']);
  assert.throws(() => core.buildExplainArgs('f', { authorityTrust: 't' }), /REQUIRES_RESULT/);
});
test('request/status projections use the selected saved identity, not arbitrary typed digests', () => {
  const request = { apiVersion: core.CLIENT_API_VERSION, kind: 'FeedbackDraft', schemaVersion: 'feedback-draft-v1', status: 'PROPOSED', requestId: 'sha256:' + '1'.repeat(64), digest: 'sha256:' + '2'.repeat(64) };
  const record = { ...core.projectSavedRequest(request), path: '/workspace/saved.json' };
  assert.equal(core.selectRequest([record], record.path, 'feedback'), record);
  assert.throws(() => core.selectRequest([record], record.path, 'exception'), /STALE_REQUEST/);
  assert.throws(() => core.projectSavedRequest({ ...request, sourceCode: 'secret' }), /FORBIDDEN/);
  for (const selector of ['requestId', 'artifactDigest']) assert.equal(core.statusSelection(record, selector)[selector], record[selector]);
  assert.throws(() => core.statusSelection(record, 'anything'), /STALE_STATUS/);
  assert.throws(() => core.buildSubmissionStatusArgs({ server: 'http://127.0.0.1:1', requestId: 'a', artifactDigest: 'b' }), /AMBIGUOUS/);
});
test('receipt projection rejects malformed, unbound and authority-bearing responses', () => {
  const request = { requestId: 'sha256:' + '1'.repeat(64), artifactDigest: 'sha256:' + '2'.repeat(64) };
  const receipt = { apiVersion: core.CLIENT_API_VERSION, kind: 'SubmissionReceipt', schemaVersion: 'submission-receipt-v1', status: 'SUBMITTED', ...request, governanceOutcome: 'RECEIVED_ONLY', exceptionApproved: false, gateMutated: false, scmPublished: false };
  assert.equal(core.validateSubmissionResponse(receipt, request), receipt);
  assert.throws(() => core.validateSubmissionResponse({}, request), /INVALID_RECEIPT/);
  assert.throws(() => core.validateSubmissionResponse({ ...receipt, gateMutated: true }, request), /GUARDS/);
  assert.throws(() => core.validateSubmissionResponse({ ...receipt, requestId: 'other' }, request), /IDENTITY/);
});


test('scope sources require a complete, closed FindingView identity', () => {
  for (const name of ['findingId', 'ruleId', 'fingerprint', 'message']) {
    const view = { ...goodView }; delete view[name]; assert.equal(core.validateFindingView(view).ok, false);
    assert.deepEqual(core.scopeCandidates(view, { modules: ['src'] }), []);
  }
  assert.equal(core.validateFindingView({ ...goodView, location: { ...goodView.location, extra: 1 } }).ok, false);
});
