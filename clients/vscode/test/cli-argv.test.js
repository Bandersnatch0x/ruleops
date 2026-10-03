'use strict';
// 阶段 4：31/32 项 CLI 的统一命令面 argv 覆盖测试。
// 两端消费同一份 clients/ide-command-map.json#commands[].params；
// 本测试证明 generic builder（VS Code core.js）能对每个 map 命令机械构造 argv，
// 并对必填/枚举/digest/路线互斥/base-head 成对做 fail-closed 校验。离线零依赖。

const test = require('node:test');
const assert = require('node:assert/strict');
const core = require('../src/core');
const commandMap = require('../../ide-command-map.json');

const SHA = 'sha256:' + 'a'.repeat(64);
const HEX = 'b'.repeat(64);

// 每个命令的最小合法字段集（对 generic builder 成立）。
function validValues(cmd) {
  const v = {};
  for (const p of cmd.params || []) {
    if (p.kind === 'bool') { v[p.name] = p.required ? true : false; continue; }
    if (p.kind === 'enum') { v[p.name] = p.values[0]; continue; }
    switch (p.kind) {
      case 'digest': v[p.name] = SHA; break;
      case 'number': v[p.name] = '1'; break;
      case 'csv': v[p.name] = 'a,b'; break;
      case 'json': v[p.name] = '{}'; break;
      default: v[p.name] = p.flag ? 'value' : (p.kind === 'path' || p.kind === 'file' || p.kind === 'dir' ? 'src/Main.java' : 'value');
    }
    if (p.hexDigest && p.kind === 'digest') v[p.name] = HEX;
  }
  if (cmd.commandId === 'rf.scan') {
    v.route = 'rule-id';
    v.ruleId = 'corp.java.smoke';
    v.candidate = undefined;
    v.deliveryDescriptor = undefined;
    v.base = v.head = undefined;
  }
  return v;
}

test('map exposes typed params + consumption marker for every command', () => {
  assert.equal(commandMap.commands.length, 32);
  assert.ok(commandMap.consumption && commandMap.consumption.consumers.length === 2);
  for (const cmd of commandMap.commands) {
    assert.ok(Array.isArray(cmd.params) && cmd.params.length > 0, cmd.commandId + ' has params');
    const names = cmd.params.map(p => p.name);
    assert.equal(new Set(names).size, names.length, cmd.commandId + ' param names unique');
  }
});

test('generic builder constructs argv for every mapped command with a minimal valid input', () => {
  for (const cmd of commandMap.commands) {
    const args = core.buildMappedArgs(cmd, validValues(cmd));
    assert.ok(Array.isArray(args) && args.length > 0, cmd.commandId + ' produced argv');
    for (const p of cmd.params) {
      if (!p.required || p.kind === 'bool') continue;
      if (p.routeSelector) continue;
      const needle = p.flag || validValues(cmd)[p.name];
      assert.ok(args.includes(needle), cmd.commandId + ' argv contains required ' + (p.flag || 'positional ' + needle));
    }
  }
});

test('required fields fail closed', () => {
  const cmd = core.mapCommand(commandMap, 'rf.feedback.draft');
  assert.throws(() => core.buildMappedArgs(cmd, { finding: 'src/Main.java', classification: 'true-fp', reason: 'r' }),
    /MISSING_ARG/);
});

test('enum fields fail closed on unknown values', () => {
  const cmd = core.mapCommand(commandMap, 'rf.config.show');
  assert.throws(() => core.buildMappedArgs(cmd, { repoRoot: 'src', format: 'xml' }), /BAD_ENUM/);
});

test('digest and hex-digest fields are shape-validated', () => {
  const ev = core.mapCommand(commandMap, 'rf.decision.evaluate');
  assert.throws(() => core.buildMappedArgs(ev, { inputDigest: 'not-a-digest', signal: 1, now: 1, output: 'src/o' }), /INVALID_HEX_DIGEST/);
  const reg = core.mapCommand(commandMap, 'rf.registry.show');
  assert.throws(() => core.buildMappedArgs(reg, { resource: 'rules', id: 'x', version: '1', expectedDigest: 'nope' }), /INVALID_DIGEST/);
});

test('repeatable positionals emit one argv per item', () => {
  const cmd = core.mapCommand(commandMap, 'rf.standard.validate');
  const args = core.buildMappedArgs(cmd, { files: ['src/a.yaml', 'src/b.yaml'] });
  assert.deepEqual(args, ['standard', 'validate', 'src/a.yaml', 'src/b.yaml']);
});

test('scan routes are mutually exclusive and base/head travel as a pair', () => {
  const cmd = core.mapCommand(commandMap, 'rf.scan');
  const base = { route: 'rule-id', ruleId: 'corp.java.smoke', repoRoot: 'src' };
  const args = core.buildMappedArgs(cmd, base);
  assert.ok(args.includes('--repo-root') && args.includes('--rule-id'));
  // 两条路线同时给定属于输入冲突。
  assert.throws(() => core.buildMappedArgs(cmd, { ...base, candidate: 'src/candidate' }), /SCAN_ROUTE_CONFLICT/);
  // base 无 head / head 无 base。
  assert.throws(() => core.buildMappedArgs(cmd, { ...base, base: 'abc' }), /BASE_HEAD_MUST_BE_PAIRED/);
  assert.throws(() => core.buildMappedArgs(cmd, { ...base, head: 'def' }), /BASE_HEAD_MUST_BE_PAIRED/);
  const paired = core.buildMappedArgs(cmd, { ...base, base: 'abc', head: 'def' });
  assert.ok(paired.includes('--base') && paired.includes('--head'));
});

test('scan candidate and delivery-descriptor routes emit their own identity flags', () => {
  const cmd = core.mapCommand(commandMap, 'rf.scan');
  assert.ok(core.buildMappedArgs(cmd, { route: 'candidate', candidate: 'src/cand', repoRoot: 'src' }).includes('--candidate'));
  assert.ok(core.buildMappedArgs(cmd, { route: 'delivery-descriptor', deliveryDescriptor: 'src/desc.json', repoRoot: 'src' }).includes('--delivery-descriptor'));
});

test('identity fields that must not be auto-filled are declared on the map', () => {
  const ids = ['rf.pilot.baseline', 'rf.pilot.observe', 'rf.pilot.feedback', 'rf.pilot.governance',
    'rf.pilot.evidence.adjudicate', 'rf.feedback.draft', 'rf.exception.request'];
  for (const commandId of ids) {
    const cmd = core.mapCommand(commandMap, commandId);
    assert.ok(cmd.params.some(p => p.identity), commandId + ' declares an identity field');
  }
});

test('side-effectful authoring/ops commands keep their required positional OUTPUT', () => {
  for (const commandId of ['rf.rule.generate', 'rf.rule.test', 'rf.coverage.analyze', 'rf.pilot.report',
    'rf.center.check', 'rf.status.contract-check', 'rf.status.deployment-check']) {
    const cmd = core.mapCommand(commandMap, commandId);
    const values = validValues(cmd);
    const args = core.buildMappedArgs(cmd, values);
    assert.ok(args.includes('src/Main.java'), commandId + ' carries its required OUTPUT positional');
  }
});