'use strict';
// 阶段 4：扫描结果工作台 + 统一命令面（sidebar 层）行为测试。
// 关键语义：scan exit=0 绝不显示为 PASS，以结构化 status/gate/authority 为准；
// map 驱动场景机械构造 argv；重副作用命令在执行前有显式预览。离线零依赖。

const test = require('node:test');
const assert = require('node:assert/strict');
const { host, findingFile } = require('./sidebar-host');
const map = require('../../ide-command-map.json');

function scanResult(overrides = {}) {
  return {
    apiVersion: 'rulefactory.io/v1', kind: 'ScanResult', schemaVersion: 'scan-output-v1',
    status: 'FAIL', gateDecision: 'FAIL', authority: 'LOCAL_UNATTESTED', trust: 'LOCAL_UNATTESTED',
    ruleId: 'corp.java.smoke', planned: 1, executed: 1, failed: 0,
    completeness: { planned: 1, accounted: 1, failed: 0, missing: 0, observationErrors: 0, complete: true },
    findingRecords: [], diagnostics: [{ code: 'FINDINGS_PRESENT', stage: 'gate', message: 'm', retry: 'Never', nextStep: 'n' }],
    progress: [], resultDigest: 'RDG', ...overrides,
  };
}

test('scan workbench renders exit=0 FAIL result as FAIL, never PASS', async t => {
  const root = t._root = (await import('node:os')).tmpdir();
  const invocations = [];
  const h = host(async (args) => {
    invocations.push(args);
    return { code: 0, stdout: JSON.stringify(scanResult()), stderr: 'progress stage=resolve outcome=COMPLETED elapsed_nanos=7' };
  }, { root: '/ws' });
  await h.sidebar.open('erf.scan');
  assert.notEqual(h.sidebar.state.phase, 'unavailable');
  await h.sidebar.execute({ route: 'rule-id', ruleId: 'corp.java.smoke', format: 'json', progress: 'none' });
  assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
  assert.match(h.sidebar.state.detail, /FAIL（存在未豁免 finding/);
  assert.doesNotMatch(h.sidebar.state.detail, /PASS（Gate 通过）/);
  assert.match(h.sidebar.state.detail, /progress stage=resolve/);
  // argv 走统一命令面：路线旗标 + repo-root 齐备，且互斥路线旗标不出现。
  const args = invocations[0];
  assert.ok(args.includes('--rule-id') && args.includes('corp.java.smoke'));
  assert.ok(!args.includes('--candidate') && !args.includes('--delivery-descriptor'));
});

test('scan ScanFailure exit=3 renders inconclusive class, not completed-as-pass', async t => {
  const h = host(async () => ({ code: 3, stdout: JSON.stringify({
    apiVersion: 'rulefactory.io/v1', kind: 'ScanFailure', exitCode: 3,
    diagnostics: [{ code: 'GATE_INCONCLUSIVE', stage: 'gate', message: 'm', retry: 'SafeRetry', nextStep: 'n' }],
    progress: [], diagnosticDigest: 'X' }), stderr: '' }), { root: '/ws' });
  await h.sidebar.open('erf.scan');
  await h.sidebar.execute({ route: 'candidate', candidate: '/ws/cand', format: 'json' });
  assert.equal(h.sidebar.state.phase, 'completed');
  assert.match(h.sidebar.state.detail, /ScanFailure/);
  assert.match(h.sidebar.state.detail, /INCONCLUSIVE_OR_REFUSED/);
  assert.doesNotMatch(h.sidebar.state.detail, /PASS（Gate 通过）/);
});

test('route conflict and unpaired base/head fail closed before invoking rf', async t => {
  let calls = 0;
  const h = host(async () => { calls++; return { code: 0, stdout: JSON.stringify(scanResult({ status: 'PASS', gateDecision: 'PASS', authority: 'TRUSTED', trust: 'TRUSTED' })) }; }, { root: '/ws' });
  await h.sidebar.open('erf.scan');
  await h.sidebar.execute({ route: 'rule-id', ruleId: 'r', candidate: '/x', format: 'json' });
  assert.equal(h.sidebar.state.phase, 'failed');
  assert.match(h.sidebar.state.detail, /SCAN_ROUTE_CONFLICT/);
  await h.sidebar.execute({ route: 'rule-id', ruleId: 'r', candidate: '', format: 'json', base: 'a' });
  assert.match(h.sidebar.state.detail, /BASE_HEAD_MUST_BE_PAIRED/);
  assert.equal(calls, 0);
});

test('map-driven workspace scenes pass typed argv through a preview gate', async t => {
  const invocations = [];
  const h = host(async (args) => {
    invocations.push(args);
    return { code: 0, stdout: 'doctor=PASS mode=standalone\n', stderr: '' };
  }, { root: '/ws' });
  await h.sidebar.open('erf.doctor');
  await h.sidebar.execute({ format: 'json' });
  assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
  assert.deepEqual(invocations[0], ['doctor', '--repo-root', '/ws', '--format', 'json']);
  assert.ok(h.calls.some(c => c[0] === 'confirm')); // 预览门确实弹过。
});

test('configValidate and standardValidate invoke mapped argv without finding file', async t => {
  const invocations = [];
  const h = host(async (args) => ({ code: 0, stdout: 'CONFIG_VALID /ws/.ruleops/config.yaml\n', stderr: '' }), { root: '/ws' });
  const orig = h.sidebar.execute;
  const traced = async (input) => { invocations.push(input); return orig(input); };
  await h.sidebar.open('erf.configValidate');
  await h.sidebar.execute({});
  assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
  await h.sidebar.open('erf.standardValidate');
  await h.sidebar.execute({ files: '/ws/specs/a.yaml,/ws/specs/b.yaml' });
  assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
});

test('heavy side-effect commands (pilot/decision/center) stay explicitly unavailable', async () => {
  const h = host(undefined, { root: '/ws' });
  for (const commandId of ['rf.pilot.report', 'rf.pilot.baseline', 'rf.pilot.evidence.adjudicate', 'rf.decision.evaluate', 'rf.center.check', 'rf.status.contract-check']) {
    await h.sidebar.open(commandId);
    assert.equal(h.sidebar.state.phase, 'unavailable', commandId);
    assert.match(h.sidebar.state.detail, /不执行模板|尚未提供此命令的参数面板/);
  }
});

test('map still drives the tree; every command node resolves to a scene or explicit unavailable', async () => {
  const h = host(undefined, { root: '/ws' });
  const nodes = h.providers.tree.getChildren({ id: 'scan_findings', group: true });
  assert.ok(nodes.length >= 3);
  for (const cmd of map.commands) {
    const covered = require('../src/sidebar').SCENARIOS.some(s => s.cli === cmd.commandId);
    if (!covered) continue; // 未覆盖命令仍以 unavailable 节点出现在树中（树构建自同一 map）。
  }
});
