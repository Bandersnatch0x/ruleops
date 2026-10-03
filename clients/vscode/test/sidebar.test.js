'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { SCENARIOS, validateMessage } = require('../src/sidebar');
const { runRf } = require('../src/rf-invoker');
const { createHash } = require('node:crypto');
const map = require('../../ide-command-map.json');
const manifest = require('../package.json');

const { host, findingFile } = require('./sidebar-host');

function requestArtifact(kind = 'feedback', extra = {}) {
  return { apiVersion: 'rulefactory.io/v1', kind: kind === 'feedback' ? 'FeedbackDraft' : 'ExceptionRequest', schemaVersion: kind + (kind === 'feedback' ? '-draft-v1' : '-request-v1'),
    status: kind === 'feedback' ? 'PROPOSED' : 'REQUESTED', requestId: 'sha256:' + '1'.repeat(64), digest: 'sha256:' + '2'.repeat(64), findingDigest: 'sha256:' + '3'.repeat(64), findingFingerprint: 'fp', ruleId: 'r',
    actor: 'confirmed-user', reason: 'reason with spaces', ...(kind === 'feedback' ? { classification: 'true-fp' } : { scope: 'src/Main.java' }), ...extra };
}
function receiptFor(request, statusQuery = false) {
  return { apiVersion: 'rulefactory.io/v1', kind: statusQuery ? 'SubmissionStatus' : 'SubmissionReceipt', schemaVersion: statusQuery ? 'submission-status-v1' : 'submission-receipt-v1',
    status: statusQuery ? 'RECEIVED_ONLY' : 'SUBMITTED', requestId: request.requestId, artifactDigest: request.digest, serverRequestId: 'server-1',
    ...(statusQuery ? { authority: 'RECEIVED_ONLY', automaticRetry: false } : { governanceOutcome: 'RECEIVED_ONLY', exceptionApproved: false, gateMutated: false, scmPublished: false }) };
}
function savedOptions(t, kind = 'feedback') {
  const file = findingFile(t); const root = path.dirname(file); const output = path.join(root, 'saved-request.json');
  const artifact = requestArtifact(kind); const content = JSON.stringify(artifact); fs.writeFileSync(output, content);
  const record = { root, path: output, kind, requestId: artifact.requestId, artifactDigest: artifact.digest, contentDigest: createHash('sha256').update(content).digest('hex'), status: artifact.status, savedAt: '2026-10-03T00:00:00Z' };
  return { file, root, output, records: [record], artifact, config: { serverEndpoint: 'http://127.0.0.1:1234/v1' } };
}
function saveCli(args) {
  const value = flag => args[args.indexOf(flag) + 1];
  const artifact = requestArtifact(args[0], { actor: value('--actor'), reason: value('--reason'), [args[0] === 'feedback' ? 'classification' : 'scope']: value(args[0] === 'feedback' ? '--classification' : '--scope') });
  assert.ok(args.includes('--output')); fs.writeFileSync(value('--output'), JSON.stringify(artifact));
  return { code: 0, stdout: '', stderr: '' };
}

test('native container, grouped tree, menus and all 31 metadata entries are discoverable', () => {
  assert.equal(manifest.contributes.viewsContainers.activitybar[0].id, 'erf');
  assert.equal(manifest.contributes.views.erf[1].type, 'webview');
  assert.ok(manifest.contributes.menus['view/title'].some(m => m.command === 'erf.cancel'));
  const h = host(); const tree = h.providers.tree;
  assert.deepEqual(tree.getChildren().slice(0, -1).map(s => s.id), Object.keys(map.sections));
  const items = tree.getChildren().flatMap(s => tree.getChildren(s));
  assert.equal(items.length, 33); // 31 CLI entries + show/navigate local scenes.
  for (const cli of map.commands) assert.ok(items.some(s => s.cli === cli.commandId || s.commandId === cli.commandId));
});

test('the actual activation binds all eight commands to the same sidebar without executing', async () => {
  require('../scripts/prepare-resources');
  const Module = require('node:module'); const original = Module._load;
  const h = host();
  Module._load = function(id, ...args) { return id === 'vscode' ? h.vscode : original.call(this, id, ...args); };
  try {
    delete require.cache[require.resolve('../src/extension')];
    const sidebar = require('../src/extension').activate(h.context);
    for (const scene of SCENARIOS) {
      await h.commands.get(scene.id)();
      assert.notEqual(sidebar.state.phase, 'running');
      assert.ok(h.calls.some(c => c[0] === 'reveal' && c[1] === scene.id));
    }
  } finally { Module._load = original; }
});

test('messages reject arbitrary paths, commands, unknown fields and prototype keys', () => {
  for (const msg of [{ type: 'run', fields: { file: '/secret' } }, { type: 'run', fields: {}, command: 'rf scan' }, { type: 'exec' }, { type: 'run', fields: [] }, JSON.parse('{"type":"run","fields":{"__proto__":"x"}}')])
    assert.throws(() => validateMessage(msg, SCENARIOS[3]));
});

test('webview has strict CSP, escaped output and no local resource access', async () => {
  const h = host(undefined, { folders: [] });
  const webview = { onDidReceiveMessage(fn) { this.message = fn; return { dispose() {} }; } };
  h.providers.webview.resolveWebviewView({ webview, onDidDispose() { return { dispose() {} }; } });
  assert.match(webview.html, /default-src 'none'/); assert.match(webview.html, /script-src 'nonce-/);
  assert.deepEqual(webview.options.localResourceRoots, []);
  assert.doesNotMatch(webview.html, /unsafe-inline|https?:\/\//);
  assert.equal(h.sidebar.state.phase, 'unconfigured');
  await h.sidebar.open('rf.init'); assert.notEqual(h.sidebar.state.phase, 'ready');
});

test('multi-root has no implicit first root; unsupported commands remain unavailable', async () => {
  const h = host(undefined, { folders: [{ uri: { fsPath: '/a' } }, { uri: { fsPath: '/b' } }] });
  assert.equal(h.sidebar.state.phase, 'unconfigured');
  await h.sidebar.message({ type: 'workspace' });
  assert.equal(h.sidebar.state.phase, 'ready');
  await h.sidebar.open('rf.scan'); assert.equal(h.sidebar.state.phase, 'unavailable');
});

test('finding privacy rejection prevents invocation; saved request submits the exact output and queries without typing digests', async t => {
  let calls = 0;
  const rejected = host(async () => { calls++; }, { file: findingFile(t, { sourceCode: 'private' }) });
  await rejected.sidebar.open('erf.explainFinding'); await rejected.sidebar.message({ type: 'pickFile' }); await rejected.sidebar.execute({});
  assert.equal(calls, 0); assert.equal(rejected.sidebar.state.phase, 'failed');
  const file = findingFile(t); const options = { file, config: { serverEndpoint: 'http://127.0.0.1:1234/v1' } }; const invocations = [];
  const h = host(async args => {
    invocations.push(args);
    if (args[1] === 'request') return saveCli(args);
    const artifact = JSON.parse(fs.readFileSync(path.join(path.dirname(file), 'saved-request.json')));
    return { code: 0, stdout: JSON.stringify(receiptFor(artifact, args[0] === 'submission')), stderr: '' };
  }, options);
  await h.sidebar.open('erf.requestException'); await h.sidebar.message({ type: 'pickFile' });
  await h.sidebar.execute({ actor: 'confirmed-user', reason: 'reason with spaces' });
  assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
  const output = path.join(path.dirname(file), 'saved-request.json');
  assert.deepEqual(invocations[0].slice(3), ['--scope', 'src/Main.java', '--reason', 'reason with spaces', '--actor', 'confirmed-user', '--output', output]);
  assert.match(h.sidebar.state.detail, /已保存/); assert.ok(h.sidebar.state.detail.includes(output));
  await h.sidebar.open('erf.submitException'); await h.sidebar.execute({});
  assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail); assert.equal(invocations[1][2], output);
  assert.equal(h.sidebar.requests[0].receipt.status, 'SUBMITTED');
  await h.sidebar.open('erf.submissionStatus'); await h.sidebar.execute({});
  assert.ok(invocations[2].includes('--request-id')); assert.ok(invocations[2].includes(h.sidebar.requests[0].requestId));
  await h.sidebar.execute({ selector: 'artifactDigest' });
  assert.ok(invocations[3].includes('--artifact-digest')); assert.ok(invocations[3].includes(h.sidebar.requests[0].artifactDigest));
});

test('missing endpoint, CLI refusal, unavailable binary and receipt guards are visible', async t => {
  const missing = host(); await missing.sidebar.open('erf.submissionStatus'); assert.equal(missing.sidebar.state.phase, 'unconfigured');
  const options = savedOptions(t);
  for (const [result, phase] of [
    [{ code: 3, stdout: '', stderr: 'pending' }, 'failed'],
    [{ code: -1, reason: 'UNAVAILABLE', stderr: 'ENOENT' }, 'unavailable'],
    [{ code: 0, stdout: JSON.stringify({ ...receiptFor(options.artifact, true), gateMutated: true }) }, 'failed'],
    [{ code: 0, stdout: 'invalid-json' }, 'failed'],
    [{ code: 0, stdout: '{}' }, 'failed'],
    [{ code: 0, stdout: JSON.stringify(receiptFor(options.artifact, true)) }, 'completed'],
  ]) {
    const h = host(async () => result, options);
    await h.sidebar.open('erf.submissionStatus'); await h.sidebar.execute({ request: options.output, selector: 'requestId' });
    assert.equal(h.sidebar.state.phase, phase, h.sidebar.state.detail);
  }
});

test('running cancels, prevents duplicate invocation and discards stale configuration result', { timeout: 5000 }, async t => {
  let release; let calls = 0; let signal; let started;
  let ready = new Promise(resolve => { started = resolve; });
  const options = savedOptions(t);
  const h = host((a,c,r,s) => { calls++; signal=s; started(); return new Promise(resolve => { release=resolve; }); }, options);
  await h.sidebar.open('erf.submissionStatus'); const run = h.sidebar.execute({ request: options.output, selector: 'requestId' }); await ready;
  assert.equal(h.sidebar.state.phase, 'running'); await h.sidebar.execute({ request: options.output, selector: 'requestId' }); assert.equal(calls, 1);
  await h.sidebar.message({ type: 'cancel' }); assert.equal(signal.aborted, true);
  release({ code: 0, stdout: '{}' }); await run; assert.equal(h.sidebar.state.phase, 'cancelled');
  ready = new Promise(resolve => { started = resolve; });
  const stale = h.sidebar.execute({ request: options.output, selector: 'requestId' }); await ready; h.conf.serverEndpoint = ''; h.changed();
  release({ code: 0, stdout: JSON.stringify(receiptFor(options.artifact, true)) }); await stale;
  assert.equal(h.sidebar.state.phase, 'unconfigured'); assert.doesNotMatch(h.sidebar.state.detail, /RECEIVED_ONLY/);
});

test('process invoker preserves argv and exposes missing binary, timeout, output limit and cancellation', { timeout: 10000 }, async () => {
  const cfg = { rfPath: process.execPath, requestTimeoutMs: 3000 };
  const echo = await runRf(['-e', 'console.log(process.argv[1])', 'spaces;not-a-shell'], cfg, process.cwd());
  assert.equal(echo.stdout.trim(), 'spaces;not-a-shell');
  assert.equal((await runRf([], { ...cfg, rfPath: '/missing/erf-test-binary' }, process.cwd())).reason, 'UNAVAILABLE');
  assert.equal((await runRf(['-e', 'setInterval(()=>{},1000)'], { ...cfg, requestTimeoutMs: 60 }, process.cwd())).reason, 'TIMEOUT');
  assert.equal((await runRf(['-e', 'process.stdout.write(Buffer.alloc(1100000))'], cfg, process.cwd())).reason, 'OUTPUT_LIMIT');
  const abort = new AbortController();
  const running = runRf(['-e', 'setInterval(()=>{},1000)'], cfg, process.cwd(), abort.signal);
  abort.abort(); assert.equal((await running).reason, 'CANCELLED');
});


test('IntelliJ source contract: registration and eight delegating actions, no modal second flow', () => {
  const root = path.resolve(__dirname, '../../intellij');
  const xml = fs.readFileSync(path.join(root, 'src/main/resources/META-INF/plugin.xml'), 'utf8');
  assert.match(xml, /<toolWindow id="ERF"[^>]+ErfToolWindowFactory/);
  assert.match(xml, /<projectConfigurable[^>]+ErfConfigurable/);
  const actions = ['ShowFinding', 'NavigateToLocation', 'ExplainFinding', 'DraftFeedback', 'RequestException', 'SubmitFeedback', 'SubmitException', 'SubmissionStatus'];
  for (const action of actions) {
    const source = fs.readFileSync(path.join(root, 'src/main/java/io/ruleops/ide/actions', action + 'Action.java'), 'utf8');
    assert.match(source, /extends AbstractErfAction/); assert.match(source, /super\(Scene\./);
    assert.doesNotMatch(source, /actionPerformed|showInputDialog|runRf|Messages\./);
  }
  const base = fs.readFileSync(path.join(root, 'src/main/java/io/ruleops/ide/actions/AbstractErfAction.java'), 'utf8');
  assert.match(base, /final void actionPerformed/); assert.match(base, /ErfToolWindowFactory.open/);
  const invoker = fs.readFileSync(path.join(root, 'src/main/java/io/ruleops/ide/RfInvoker.java'), 'utf8');
  assert.match(invoker, /new Task.Backgroundable/); assert.match(invoker, /indicator.checkCanceled\(\)/); assert.match(invoker, /assertIsDispatchThread\(\)/);
  // This is source/packaging evidence, NOT an SDK host test or a real IDE acceptance claim.
});


test('navigation uses the shared path gate and rejects symlink components before opening', async t => {
  const file = findingFile(t); const root = path.dirname(file);
  fs.mkdirSync(path.join(root, 'src'), { recursive: true }); fs.writeFileSync(path.join(root, 'src/Main.java'), 'class Main {}');
  const h = host(undefined, { root, file }); let opened;
  h.vscode.workspace.openTextDocument = async uri => { opened = uri.fsPath; return {}; };
  h.vscode.window.showTextDocument = async () => ({ revealRange() {} });
  h.vscode.Position = class { constructor(line, column) { this.line=line; this.column=column; } };
  h.vscode.Selection = class {}; h.vscode.Range = class {};
  await h.sidebar.open('erf.navigateToLocation'); await h.sidebar.message({ type: 'pickFile' });
  await h.sidebar.execute({}); assert.equal(h.sidebar.state.phase, 'completed'); assert.equal(opened, path.join(root, 'src/Main.java'));
  fs.renameSync(path.join(root, 'src'), path.join(root, 'real-src'));
  fs.symlinkSync(path.join(root, 'real-src'), path.join(root, 'src'), 'junction'); opened = undefined;
  await h.sidebar.execute({}); assert.equal(h.sidebar.state.phase, 'failed'); assert.equal(opened, undefined);
  assert.match(h.sidebar.state.detail, /SYMLINK_COMPONENT/);
});


test('validated form edits survive host renders but not a workspace switch', async () => {
  const h = host(); await h.sidebar.open('erf.draftFeedback');
  const webview = { onDidReceiveMessage() { return { dispose() {} }; } };
  h.providers.webview.resolveWebviewView({ webview, onDidDispose() { return { dispose() {} }; } });
  await h.sidebar.message({ type: 'edit', fields: { actor: 'confirmed-person', reason: '<script>private text</script>' } });
  await h.sidebar.message({ type: 'refresh' });
  assert.match(webview.html, /confirmed-person/); assert.match(webview.html, /&lt;script&gt;private text/);
  await h.sidebar.message({ type: 'workspace' }); assert.doesNotMatch(webview.html, /confirmed-person/);
});


test('dynamic scope dropdown uses only current verified config; stale selection never picks the first item', async t => {
  const file = findingFile(t); const root = path.dirname(file); const options = { file, effective: { modules: ['src'], source_roots: [] } };
  const h = host(saveCli, options); await h.sidebar.open('erf.requestException'); await h.sidebar.message({ type: 'pickFile' });
  const webview = { onDidReceiveMessage() { return { dispose() {} }; } }; h.providers.webview.resolveWebviewView({ webview, onDidDispose() { return { dispose() {} }; } });
  assert.match(webview.html, /select name="scope"/); assert.doesNotMatch(webview.html, /textarea name="scope"/);
  await h.sidebar.message({ type: 'edit', fields: { scope: 'removed/**', actor: 'person', reason: 'private reason' } });
  await h.sidebar.message({ type: 'refresh' }); assert.match(webview.html, /已失效/);
  assert.match(webview.html, /button type="submit" disabled/);
  await h.sidebar.execute({}); assert.equal(h.sidebar.state.phase, 'failed'); assert.match(h.sidebar.state.detail, /STALE_SCOPE/);
  assert.equal(fs.existsSync(path.join(root, 'saved-request.json')), false);
});

test('save cancellation and output failure preserve form and never claim an artifact was created', async t => {
  for (const failure of ['cancelSave', 'cancelConfirm', 'no-output', 'write-error']) {
    const file = findingFile(t); const options = { file, [failure]: true }; let calls = 0;
    const h = host(async () => { calls++; return failure === 'write-error' ? { code: 2, stderr: 'permission denied' } : { code: 0, stdout: '{}' }; }, options);
    await h.sidebar.open('erf.draftFeedback'); await h.sidebar.message({ type: 'pickFile' });
    const webview = { onDidReceiveMessage() { return { dispose() {} }; } }; h.providers.webview.resolveWebviewView({ webview, onDidDispose() { return { dispose() {} }; } });
    await h.sidebar.execute({ classification: 'documentation', actor: 'confirmed-user', reason: 'keep this reason' });
    assert.equal(h.sidebar.state.phase, failure.startsWith('cancel') ? 'cancelled' : 'failed');
    assert.equal(calls, failure.startsWith('cancel') ? 0 : 1);
    assert.equal(h.sidebar.requests.length, 0); assert.doesNotMatch(h.sidebar.state.detail, /已保存|已生成申请|已起草/);
    assert.match(webview.html, /keep this reason/); assert.match(webview.html, /confirmed-user/);
  }
});

test('modified/deleted/symlink requests are invalidated before submit and endpoint changes clear selection', async t => {
  for (const change of ['modified', 'deleted', 'symlink']) {
    const options = savedOptions(t); let invoked = false; const h = host(async () => { invoked = true; }, options);
    if (change === 'modified') fs.appendFileSync(options.output, ' ');
    if (change === 'deleted') fs.unlinkSync(options.output);
    if (change === 'symlink') { fs.renameSync(options.output, options.output + '.real'); fs.symlinkSync(options.output + '.real', options.output); }
    await h.sidebar.open('erf.submitFeedback'); await h.sidebar.execute({ request: options.output });
    assert.equal(h.sidebar.state.phase, 'failed'); assert.equal(invoked, false);
  }
  const options = savedOptions(t); const h = host(async () => ({ code: 0, stdout: JSON.stringify(receiptFor(options.artifact)) }), options);
  await h.sidebar.open('erf.submitFeedback'); await h.sidebar.execute({ request: options.output });
  assert.equal(h.sidebar.requests.length, 1);
  h.conf.serverEndpoint = 'http://127.0.0.1:9999/v1'; h.changed();
  assert.equal(h.sidebar.requests.length, 0);
  await h.sidebar.execute({ request: options.output }); assert.equal(h.sidebar.state.phase, 'failed'); assert.match(h.sidebar.state.detail, /STALE_REQUEST/);
});

test('scope expansion and overwrite require explicit confirmation; actor is never guessed', async t => {
  const file = findingFile(t); const h = host(saveCli, { file });
  await h.sidebar.open('erf.requestException'); await h.sidebar.message({ type: 'pickFile' });
  await h.sidebar.execute({ scope: 'src/**', actor: 'person', reason: 'why' });
  assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
  assert.ok(h.calls.some(call => call[0] === 'confirm' && call[1].includes('扩大范围') && call[1].includes('actor：person')));
  await h.sidebar.execute({ scope: 'src/**', actor: 'person', reason: 'why' });
  assert.ok(h.calls.some(call => call[0] === 'confirm' && call[1].includes('覆盖已存在文件')));
});


test('explicit output may be outside workspace but never overwrites Finding/source or follows symlinks', async t => {
  const file = findingFile(t); const other = findingFile(t); const output = path.join(path.dirname(other), 'outside-request.json');
  const h = host(saveCli, { file, output }); await h.sidebar.open('erf.draftFeedback'); await h.sidebar.message({ type: 'pickFile' });
  await h.sidebar.execute({ actor: 'person', reason: 'why' });
  assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail); assert.equal(h.sidebar.requests[0].path, output);
  for (const target of [file, path.join(path.dirname(file), 'src/Main.java')]) {
    const original = fs.readFileSync(target, 'utf8'); let invoked = false;
    const blocked = host(async () => { invoked = true; }, { file, output: target });
    await blocked.sidebar.open('erf.draftFeedback'); await blocked.sidebar.message({ type: 'pickFile' });
    await blocked.sidebar.execute({ actor: 'person', reason: 'why' });
    assert.equal(blocked.sidebar.state.phase, 'failed'); assert.match(blocked.sidebar.state.detail, /OUTPUT_OVERWRITES_INPUT/);
    assert.equal(invoked, false); assert.equal(fs.readFileSync(target, 'utf8'), original);
  }
  const link = path.join(path.dirname(file), 'output-link.json'); fs.symlinkSync(output, link);
  const blocked = host(saveCli, { file, output: link }); await blocked.sidebar.open('erf.draftFeedback'); await blocked.sidebar.message({ type: 'pickFile' });
  await blocked.sidebar.execute({ actor: 'person', reason: 'why' }); assert.equal(blocked.sidebar.state.phase, 'failed'); assert.match(blocked.sidebar.state.detail, /SYMLINK/);
});
