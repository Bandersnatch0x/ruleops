'use strict';
// 阶段 5：阶段 3/4 新增面的负例与边界测试（离线零依赖）。
// 覆盖：registry NUL cursor 与分页边界、PIN mismatch/缺独立 pin、scope 扩展确认、
// submission status UNKNOWN 查询、schema 非法、路径越界（..\、绝对、控制字符）、写文件失败。

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const core = require('../src/core');
const { host, findingFile, attachWebview } = require('./sidebar-host');

const digest = 'sha256:' + 'a'.repeat(64);
const other = 'sha256:' + 'b'.repeat(64);
const pin = { resource: 'rules', id: 'corp.java.rule', version: '1.0.0', expectedDigest: digest, provenance: 'release 7' };
const connection = { connectionProfile: '/profiles/p.json', timeoutMs: 5000 };
const item = { type: 'rules', id: pin.id, version: pin.version, state: 'PUBLISHED', digest };
const envelope = data => ({ status: 'RECEIVED_ONLY', authority: 'UNKNOWN', digestVerification: 'FORMAT_ONLY', registry: {
  apiVersion: 'rulefactory.io/v1', schemaVersion: 'asset-registry-v1', authority: 'UNKNOWN', requestId: 'registry-read', ...data } });
const page = (cursor = '', nextCursor = 'n\0x', hasMore = true) => envelope({ cursor, nextCursor, hasMore, items: [item] });
const show = (extra = {}) => ({ ...envelope({ resource: { ...item, ...extra } }), digestVerification: 'PIN_MATCH' });
const ok = receipt => ({ code: 0, stdout: JSON.stringify(receipt), stderr: '' });

// ---- registry：NUL cursor 与分页边界 ----
test('registry cursor edge cases: NUL preserved, hasMore=false stops paging, page echo mismatch fails closed', () => {
  const cursor = 'a\0b"\\c\nd'; // 控制（NUL/换行）+ 引号 + 反斜杠混合。
  const args = core.buildRegistryListArgs('rules', { ...connection, limit: 10, cursor });
  assert.equal(JSON.parse(args[args.indexOf('--cursor-json') + 1]), cursor);
  assert.ok(args.every(value => !/[\0\r\n]/.test(value)), 'argv stays NUL/newline free');
  // hasMore=false 时末页正常接受（nextCursor 无进一步语义）。
  assert.equal(core.projectRegistryPage(page('', 'end', false), 'rules').hasMore, false);
  // 回显 cursor 与请求不一致 → 拒绝（防串页）。
  assert.throws(() => core.projectRegistryPage(page('other', 'n2', true), 'rules'), /PAGE_INVALID/);
  // hasMore=true 但 items 为空 → 拒绝。
  assert.throws(() => core.projectRegistryPage(envelope({ cursor: '', nextCursor: 'n', hasMore: true, items: [] }), 'rules'), /PAGE_INVALID/);
});

// ---- registry：PIN mismatch / 缺独立 pin / schema 非法 ----
test('registry pin mismatch and missing independent pin fail closed with precise errors', () => {
  // 摘要不匹配。
  assert.throws(() => core.renderRegistryShow(show({ digest: other }), pin), /DIGEST_MISMATCH/);
  // 身份不匹配（version 换了）。
  assert.throws(() => core.renderRegistryShow(show({ version: '9.9.9' }), pin), /IDENTITY_MISMATCH/);
  // 未选独立 pin 就执行 show/sync。
  assert.throws(() => core.selectRegistryPin([], '', 'rules'), /INDEPENDENT_PIN_REQUIRED/);
  assert.throws(() => core.selectRegistryPin([pin], '0', 'standards'), /INDEPENDENT_PIN_REQUIRED/);
  // release JSON schema 非法：pins 非数组 / pin 非对象 / digest 非 sha256 / provenance 带控制字符。
  assert.throws(() => core.projectRegistryPins({ pins: 'x' }), /INVALID_RELEASE_PINS/);
  assert.throws(() => core.projectRegistryPins({ pins: [null] }), /INVALID_RELEASE_PIN/);
  assert.throws(() => core.projectRegistryPins({ pins: [{ ...pin, expectedDigest: 'md5:zz' }] }), /INVALID_EXPECTED_DIGEST/);
  assert.throws(() => core.projectRegistryPins({ pins: [{ ...pin, provenance: 'rel\0ease' }] }), /INVALID_REGISTRY_FIELD/);
  // sync 只允许 rules。
  assert.throws(() => core.buildRegistrySyncArgs({ ...pin, resource: 'standards' }, '/d.json', '/repo', connection), /SYNC_RULES_ONLY/);
});

// ---- registry：endpoint 非法（鉴权信息内嵌/查询串/哈希）----
test('registry endpoint rejects userinfo, query, hash and non-http schemes', () => {
  for (const bad of ['https://u:p@example.com/v1', 'http://127.0.0.1/v1?token=x', 'https://example.com/v1#frag', 'ftp://example.com']) {
    assert.throws(() => core.buildRegistryListArgs('rules', { server: bad, limit: 1, timeoutMs: 1000 }), /ENDPOINT|INVALID/,
      'should reject ' + bad);
  }
});

// ---- submission status：UNKNOWN 查询回执 fail closed ----
test('submission status UNKNOWN receipt is accepted as UNKNOWN, never rendered as success', () => {
  const request = { requestId: digest, artifactDigest: other };
  const status = { apiVersion: 'rulefactory.io/v1', kind: 'SubmissionStatus', schemaVersion: 'submission-status-v1',
    status: 'UNKNOWN', authority: 'UNKNOWN', automaticRetry: false, requestId: digest };
  assert.equal(core.validateSubmissionResponse(status, request, true), status);
  assert.match(core.statusLabel('UNKNOWN'), /UNKNOWN/);
  assert.match(core.statusLabel('UNKNOWN'), /不作成功处理/);
  // Java 核（checkedResponse）按 status 推导 authority（UNKNOWN→必须 UNKNOWN）并抛
  // STATUS_AUTHORITY_GUARD；JS 核只校验 authority ∈ {UNKNOWN, RECEIVED_ONLY}。
  // 这里固定当前 JS 契约；该分歧已写入阶段 5 报告（两端 status 查询各自渲染，不在 parity harness 内）。
  assert.deepEqual(core.validateSubmissionResponse({ ...status, authority: 'RECEIVED_ONLY' }, request, true),
    { ...status, authority: 'RECEIVED_ONLY' });
  // automaticRetry=true → 拒绝。
  assert.throws(() => core.validateSubmissionResponse({ ...status, automaticRetry: true }, request, true), /INVALID_STATUS_RECEIPT/);
  // status 非契约值 → 拒绝。
  assert.throws(() => core.validateSubmissionResponse({ ...status, status: 'APPROVED' }, request, true), /INVALID_STATUS_RECEIPT/);
  // 身份字段全缺 → 拒绝。
  const noIdentity = { ...status }; delete noIdentity.requestId;
  assert.throws(() => core.validateSubmissionResponse(noIdentity, request, true), /IDENTITY_MISSING/);
});

// ---- schema 非法：receipt/status schemaVersion 错误 ----
test('receipt and status schema mismatches are rejected before rendering', () => {
  const request = { requestId: digest, artifactDigest: other };
  const receipt = { apiVersion: 'rulefactory.io/v1', kind: 'SubmissionReceipt', schemaVersion: 'submission-receipt-v1',
    status: 'SUBMITTED', requestId: digest, artifactDigest: other, governanceOutcome: 'RECEIVED_ONLY',
    exceptionApproved: false, gateMutated: false, scmPublished: false };
  assert.throws(() => core.validateSubmissionResponse({ ...receipt, schemaVersion: 'v0' }, request), /INVALID_RECEIPT/);
  assert.throws(() => core.validateSubmissionResponse({ ...receipt, apiVersion: 'other/v9' }, request), /INVALID_RECEIPT/);
  assert.throws(() => core.validateSubmissionResponse({ ...receipt, kind: 'SubmissionStatus' }, request), /INVALID_RECEIPT/);
});

// ---- 路径越界：..\ / 绝对 / 控制字符在 FindingView location 上 ----
test('finding location path traversal, absolute and control characters all rejected', () => {
  for (const bad of ['..\\..\\etc\\passwd', '/etc/passwd', 'C:\\win', 'a\0b', 'a\nb', 'src/../../x', 'a//b']) {
    const view = { apiVersion: 'rulefactory.io/v1', kind: 'FindingView', findingId: 'f', ruleId: 'r',
      fingerprint: 'fp', message: 'm', location: { path: bad, line: 1, column: 1 } };
    const r = core.validateFindingView(view);
    assert.equal(r.ok, false, 'should reject ' + JSON.stringify(bad));
  }
});

// ---- 写文件失败：输出不可写 / CLI exit=2 不产出请求 ----
test('write failure path: CLI error leaves no saved request and shows failed, form preserved', async t => {
  const file = findingFile(t);
  const h = host(async () => ({ code: 2, stdout: '', stderr: 'permission denied' }), { file });
  await h.sidebar.open('erf.draftFeedback');
  await h.sidebar.message({ type: 'pickFile' });
  const webview = attachWebview(h);
  await h.sidebar.execute({ classification: 'documentation', actor: 'confirmed-user', reason: 'keep' });
  assert.equal(h.sidebar.state.phase, 'failed');
  assert.match(h.sidebar.state.detail, /exit=2/);
  assert.equal(h.sidebar.requests.length, 0);
  assert.match(JSON.stringify(webview.update), /keep/);
  assert.match(JSON.stringify(webview.update), /confirmed-user/);
});

// ---- scan：schema 非法输出 fail closed，不渲染 PASS ----
test('scan workbench rejects malformed scan output and never renders PASS from garbage', async () => {
  for (const stdout of ['not-json', JSON.stringify({ kind: 'ScanResult' }), JSON.stringify({ kind: 'Mystery' }),
    JSON.stringify({ apiVersion: 'rulefactory.io/v1', kind: 'ScanResult', status: 'WEIRD', gateDecision: '', authority: '', trust: '', ruleId: 'r', findingRecords: [], diagnostics: [], progress: [], resultDigest: '' })]) {
    const h = host(async () => ({ code: 0, stdout, stderr: '' }), { root: '/ws' });
    await h.sidebar.open('erf.scan');
    await h.sidebar.execute({ route: 'rule-id', ruleId: 'r', format: 'json' });
    assert.equal(h.sidebar.state.phase, 'completed');
    assert.doesNotMatch(h.sidebar.state.detail, /PASS（Gate 通过）/, 'garbage must not render as PASS: ' + stdout.slice(0, 40));
    assert.match(h.sidebar.state.detail, /无法按既有|SCAN_(KIND_UNKNOWN|RESULT_INCOMPLETE|STATUS_UNKNOWN)/);
  }
});

// ---- scan：delivery-descriptor 路线端到端（stage4 测试只测了 rule-id/candidate）----
test('scan delivery-descriptor route emits its identity flag end to end', async () => {
  const invocations = [];
  const result = { apiVersion: 'rulefactory.io/v1', kind: 'ScanResult', schemaVersion: 'scan-output-v1',
    status: 'PASS', gateDecision: 'PASS', authority: 'TRUSTED', trust: 'TRUSTED', ruleId: 'corp.java.smoke',
    planned: 1, executed: 1, failed: 0, findingRecords: [], diagnostics: [], progress: [], resultDigest: 'RDG' };
  const h = host(async args => { invocations.push(args); return { code: 0, stdout: JSON.stringify(result), stderr: '' }; }, { root: '/ws' });
  await h.sidebar.open('erf.scan');
  await h.sidebar.execute({ route: 'delivery-descriptor', deliveryDescriptor: '/ws/desc.json', format: 'json' });
  assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
  assert.ok(invocations[0].includes('--delivery-descriptor') && invocations[0].includes('/ws/desc.json'));
  assert.ok(!invocations[0].includes('--rule-id') && !invocations[0].includes('--candidate'));
  assert.match(h.sidebar.state.detail, /PASS（Gate 通过）/);
});

// ---- registryList sidebar：limit 非法输入 fail closed（不调用 rf）----
test('registry list with non-numeric or out-of-range limit fails closed without invoking rf', async t => {
  const options = (() => {
    const file = findingFile(t); const root = path.dirname(file);
    return { root, config: { connectionProfile: connection.connectionProfile } };
  })();
  let calls = 0;
  const h = host(async () => { calls++; return ok(page()); }, options);
  await h.sidebar.open('erf.registryList');
  for (const limit of ['0', '257', 'abc', '1.5']) {
    await h.sidebar.execute({ resource: 'rules', limit });
    assert.equal(h.sidebar.state.phase, 'failed', 'limit=' + limit);
    assert.match(h.sidebar.state.detail, /LIMIT|INVALID/);
  }
  assert.equal(calls, 0);
});

// ---- NUL cursor 端到端（sidebar 层）：第二页 argv 携带 JSON 编码 cursor，不拆参数 ----
test('sidebar registry paging round-trips a hostile cursor through --cursor-json', async t => {
  const options = (() => {
    const file = findingFile(t); const root = path.dirname(file);
    return { root, config: { connectionProfile: connection.connectionProfile } };
  })();
  const hostile = 'p\0a\0"quoted"\\path 中文';
  const calls = [];
  const itemFor = resource => ({ type: resource, id: pin.id, version: pin.version, state: 'PUBLISHED', digest });
  const pageFor = (resource, cursor, nextCursor, hasMore) => envelope({ cursor, nextCursor, hasMore, items: [itemFor(resource)] });
  const h = host(async args => {
    calls.push(args);
    const resource = args[2];
    return calls.length === 1 ? ok(pageFor(resource, '', hostile, true)) : ok(pageFor(resource, hostile, 'end', false));
  }, options);
  await h.sidebar.open('erf.registryList');
  await h.sidebar.execute({ resource: 'standards', limit: '10' });
  await h.sidebar.message({ type: 'nextPage' });
  assert.equal(JSON.parse(calls[1][calls[1].indexOf('--cursor-json') + 1]), hostile);
  assert.equal(calls[1][2], 'standards'); // resource positional intact.
  assert.ok(calls[1].every(v => !v.includes('\0')));
  assert.match(h.sidebar.state.detail, /未安装/);
});
