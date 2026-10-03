'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const core = require('../src/core');
const { host, findingFile } = require('./sidebar-host');
const digest = 'sha256:' + 'a'.repeat(64);
const pin = { resource: 'rules', id: 'rule with spaces', version: 'v 1', expectedDigest: digest, provenance: 'release job 42' };
const connection = { connectionProfile: '/profiles/approved profile.json', timeoutMs: 5000 };
const item = { type: pin.resource, id: pin.id, version: pin.version, state: 'PUBLISHED', digest };
const envelope = data => ({ status: 'RECEIVED_ONLY', authority: 'UNKNOWN', digestVerification: 'FORMAT_ONLY', registry: {
  apiVersion: 'rulefactory.io/v1', schemaVersion: 'asset-registry-v1', authority: 'UNKNOWN', requestId: 'registry-read', ...data,
} });
const page = (cursor = '', nextCursor = 'opaque\0id\0v 1"\\中文', hasMore = true) => envelope({ cursor, nextCursor, hasMore, items: [item] });
const show = (extra = {}) => ({ ...envelope({ resource: { ...item, ...extra } }), digestVerification: 'PIN_MATCH' });
const sync = () => ({ status: 'CACHED', authority: 'UNKNOWN', digestVerification: 'PIN_MATCH', artifactKind: 'SignedRuleBundle',
  bundleDigest: 'sha256:' + 'b'.repeat(64), destination: '/cache/bundle.zip', overwritePolicy: 'IMMUTABLE_DIGEST_ADDRESSED', offlineAvailable: true,
  provenance: { descriptor: '/release/delivery.json', trustRoot: 'RULEOPS_TRUST_ROOT' }, registry: show().registry });
const ok = receipt => ({ code: 0, stdout: JSON.stringify(receipt), stderr: '' });
const arg = (args, flag) => args[args.indexOf(flag) + 1];
function releaseOptions(t) {
  const finding = findingFile(t); const root = path.dirname(finding); const file = path.join(root, 'release.json');
  fs.writeFileSync(file, JSON.stringify({ pins: [pin, { ...pin, version: 'v 2', expectedDigest: 'sha256:' + 'c'.repeat(64), provenance: 'release job 43', deliveryDescriptor: 'external delivery.json' }] }));
  return { root, file, config: { connectionProfile: connection.connectionProfile, serverEndpoint: 'http://127.0.0.1:9/v1' } };
}

test('core registry list preserves opaque NUL cursor as one JSON argv; validates limits and explicit connection', () => {
  const cursor = 'r\0v\0"\\中文';
  const args = core.buildRegistryListArgs('rules', { ...connection, limit: 50, cursor });
  assert.equal(arg(args, '--cursor-json'), JSON.stringify(cursor));
  assert.equal(JSON.parse(arg(args, '--cursor-json')), cursor);
  assert.ok(args.every(value => !value.includes('\0')));
  assert.equal(args.filter(value => value === '--cursor-json').length, 1);
  assert.equal(arg(args, '--connection-profile'), connection.connectionProfile);
  assert.throws(() => core.buildRegistryListArgs('rules', { ...connection, server: 'http://127.0.0.1:9/v1', limit: 50 }), /EXACTLY_ONE/);
  for (const limit of [0, 257, 1.5]) assert.throws(() => core.buildRegistryListArgs('rules', { ...connection, limit }), /LIMIT/);
  assert.throws(() => core.buildRegistryListArgs('rules', { server: 'https://user:secret@example.com/v1', limit: 1, timeoutMs: 50 }), /ENDPOINT/);
});
test('core keeps each release identity/version/digest/provenance together, never substitutes metadata', () => {
  const pins = core.projectRegistryPins({ pins: [pin, { ...pin, version: 'v 2', expectedDigest: 'sha256:' + 'c'.repeat(64) }] });
  const chosen = core.selectRegistryPin(pins, '1', 'rules');
  assert.deepEqual(core.buildRegistryShowArgs(chosen, connection), ['registry', 'show', 'rules', pin.id, '--version', 'v 2', '--expected-digest', 'sha256:' + 'c'.repeat(64), '--connection-profile', connection.connectionProfile, '--timeout-ms', '5000']);
  assert.throws(() => core.selectRegistryPin(pins, '01', 'rules'), /PIN_REQUIRED/);
  assert.throws(() => core.selectRegistryPin(pins, '0', 'standards'), /PIN_REQUIRED/);
  assert.throws(() => core.projectRegistryPins({ pins: [{ ...pin, provenance: '' }] }), /provenance/);
  assert.throws(() => core.projectRegistryPins({ pins: [item] }), /TYPE/);
});
test('core rejects mismatch/schema and labels PIN_MATCH, UNKNOWN and cache without installation claims', () => {
  assert.match(core.renderRegistryShow(show(), pin), /PIN_MATCH.*UNKNOWN.*未安装/);
  assert.match(core.renderRegistryShow({ ...show(), digestVerification: 'UNKNOWN' }, pin), /UNKNOWN.*未安装/);
  assert.throws(() => core.renderRegistryShow(show({ digest: 'sha256:' + 'c'.repeat(64) }), pin), /DIGEST_MISMATCH/);
  assert.throws(() => core.renderRegistryShow(show({ version: 'v 2' }), pin), /IDENTITY_MISMATCH/);
  assert.throws(() => core.projectRegistryPage(page(), 'standards'), /RESPONSE_INVALID/);
  assert.throws(() => core.projectRegistryPage(page('stale'), 'rules'), /PAGE_INVALID/);
  assert.match(core.renderRegistrySync(sync(), pin), /bundleDigest：sha256:b/);
  assert.match(core.renderRegistrySync(sync(), pin), /destination：\/cache\/bundle.zip/);
  assert.match(core.renderRegistrySync(sync(), pin), /IMMUTABLE_DIGEST_ADDRESSED\nofflineAvailable：true/);
  assert.match(core.renderRegistrySync({ ...sync(), offlineAvailable: false }, pin), /offlineAvailable：false/);
  assert.match(core.renderRegistrySync({ status: 'UNKNOWN', authority: 'UNKNOWN' }, pin), /离线可用性未知/);
  assert.throws(() => core.renderRegistrySync({ ...sync(), bundleDigest: undefined }, pin), /INVALID/);
});
test('core sync argv uses independent pin, external descriptor and repo root without shell splitting', () => {
  const args = core.buildRegistrySyncArgs(pin, '/signed delivery.json', '/repo with spaces', connection);
  assert.deepEqual(args.slice(0, 8), ['registry', 'sync', 'rules', pin.id, '--version', pin.version, '--expected-digest', digest]);
  assert.equal(arg(args, '--delivery-descriptor'), '/signed delivery.json');
  assert.equal(arg(args, '--repo-root'), '/repo with spaces');
  assert.throws(() => core.buildRegistrySyncArgs({ ...pin, resource: 'standards' }, '/d', '/repo', connection), /SYNC_RULES_ONLY/);
});
test('sidebar paging uses CLI cursor untouched; listing cannot create pin; profile setting is FILE only', async t => {
  const options = releaseOptions(t); const calls = []; const cursor = page().registry.nextCursor;
  const h = host(async args => { calls.push(args); return ok(calls.length === 1 ? page() : page(cursor, 'end', false)); }, options);
  await h.sidebar.open('erf.registryList');
  await h.sidebar.execute({ resource: 'rules', limit: '50' });
  assert.equal(h.sidebar.state.phase, 'completed'); assert.match(h.sidebar.state.detail, /UNKNOWN.*未安装/);
  await h.sidebar.message({ type: 'nextPage' });
  assert.equal(JSON.parse(arg(calls[1], '--cursor-json')), cursor);
  assert.equal(arg(calls[1], '--connection-profile'), connection.connectionProfile); assert.ok(!calls[1].includes('--server'));
  await h.sidebar.open('erf.registryShow'); await h.sidebar.execute({ resource: 'rules', pin: '0' });
  assert.equal(calls.length, 2); assert.match(h.sidebar.state.detail, /INDEPENDENT_PIN_REQUIRED/);
  const property = require('../package.json').contributes.configuration.properties['erf.connectionProfile'];
  assert.equal(property.type, 'string'); assert.equal(property.default, ''); assert.match(property.description, /FILE/);
});
test('sidebar independent pin dropdown and descriptor survive show mismatch, sync uses exact second record', async t => {
  const options = releaseOptions(t); const calls = [];
  const h = host(async args => {
    calls.push(args);
    if (args[1] === 'sync') return ok({ ...sync(), registry: show({ version: 'v 2', digest: 'sha256:' + 'c'.repeat(64) }).registry });
    return ok(show());
  }, options);
  let view = { webview: { html: '', onDidReceiveMessage() { return { dispose() {} }; } }, onDidDispose() { return { dispose() {} }; } };
  h.providers.webview.resolveWebviewView(view);
  await h.sidebar.open('erf.registryShow'); await h.sidebar.message({ type: 'pickReleasePins' });
  assert.match(view.webview.html, /release job 43/); assert.match(view.webview.html, /name="pin"/);
  assert.doesNotMatch(view.webview.html, /name="(?:id|version|expectedDigest)"/);
  await h.sidebar.execute({ resource: 'rules', pin: '1' });
  assert.equal(h.sidebar.state.phase, 'failed'); assert.match(h.sidebar.state.detail, /IDENTITY_MISMATCH/);
  await h.sidebar.open('erf.registrySync');
  await h.sidebar.message({ type: 'edit', fields: { resource: 'rules', pin: '1' } });
  assert.match(view.webview.html, /external delivery\.json/);
  await h.sidebar.execute({ resource: 'rules', pin: '1' });
  assert.equal(h.sidebar.state.phase, 'completed'); assert.match(h.sidebar.state.detail, /缓存非安装/);
  assert.equal(arg(calls[1], '--version'), 'v 2'); assert.equal(arg(calls[1], '--expected-digest'), 'sha256:' + 'c'.repeat(64));
  assert.equal(arg(calls[1], '--delivery-descriptor'), path.join(options.root, 'external delivery.json'));
  assert.equal(arg(calls[1], '--repo-root'), options.root);
  h.conf.connectionProfile = '/other/profile.json'; h.changed();
  await h.sidebar.execute({ resource: 'rules', pin: '1' });
  assert.equal(calls.length, 2); assert.match(h.sidebar.state.detail, /INDEPENDENT_PIN_REQUIRED/);
});
test('sidebar external descriptor picker never replaces the independent pin; failures do not fall back', async t => {
  const options = releaseOptions(t); const calls = [];
  const h = host(async args => { calls.push(args); return { code: 3, stdout: '', stderr: 'REGISTRY_DIGEST_MISMATCH' }; }, options);
  await h.sidebar.open('erf.registrySync'); await h.sidebar.message({ type: 'pickReleasePins' });
  await h.sidebar.message({ type: 'edit', fields: { resource: 'rules', pin: '0' } });
  options.file = path.join(options.root, 'external.json'); fs.writeFileSync(options.file, '{}');
  await h.sidebar.message({ type: 'pickDescriptor' });
  await h.sidebar.execute({ resource: 'rules', pin: '0' });
  assert.equal(arg(calls[0], '--delivery-descriptor'), options.file);
  assert.equal(arg(calls[0], '--expected-digest'), digest);
  assert.equal(calls.length, 1); assert.equal(h.sidebar.state.phase, 'failed'); assert.match(h.sidebar.state.detail, /UNKNOWN.*未安装.*exit=3/);
});


test('real process argv keeps the JSON-encoded NUL cursor in one argument', async () => {
  const cursor = 'id\0version "quoted"\\unicode 中文';
  const args = core.buildRegistryListArgs('standards', { ...connection, limit: 10, cursor });
  const { runRf } = require('../src/rf-invoker');
  const result = await runRf(['-e', 'process.stdout.write(JSON.stringify(process.argv.slice(1)))', ...args], { rfPath: process.execPath, requestTimeoutMs: 3000 }, process.cwd());
  assert.equal(result.code, 0, result.stderr);
  const actual = JSON.parse(result.stdout);
  assert.deepEqual(actual, args); assert.equal(JSON.parse(arg(actual, '--cursor-json')), cursor);
});
