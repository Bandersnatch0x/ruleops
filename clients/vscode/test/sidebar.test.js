'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createSidebar, SCENARIOS, validateMessage } = require('../src/sidebar');
const { runRf } = require('../src/rf-invoker');
const map = require('../../ide-command-map.json');
const manifest = require('../package.json');

function host(invoke, options = {}) {
  const commands = new Map(); const calls = []; const providers = {};
  const conf = { rfPath: 'rf', serverEndpoint: '', requestTimeoutMs: 5000, ...options.config };
  let changed; let foldersChanged;
  const disposable = () => ({ dispose() {} });
  const vscode = {
    EventEmitter: class { constructor() { this.event = () => disposable(); } fire() {} dispose() {} },
    TreeItem: class { constructor(label, state) { this.label = label; this.collapsibleState = state; } },
    Uri: { file: fsPath => ({ fsPath }) },
    commands: { registerCommand(id, fn) { commands.set(id, fn); return disposable(); }, async executeCommand(...args) { calls.push(args); } },
    window: {
      createTreeView(id, opts) { providers.tree = opts.treeDataProvider; return { ...disposable(), async reveal(node) { calls.push(['reveal', node.id]); } }; },
      registerWebviewViewProvider(id, provider) { providers.webview = provider; return disposable(); },
      async showOpenDialog() { return options.file ? [{ fsPath: options.file }] : undefined; },
      async showWorkspaceFolderPick() { return { uri: { fsPath: options.nextRoot || '/chosen' } }; },
    },
    workspace: {
      workspaceFolders: options.folders || [{ uri: { fsPath: options.root || '/workspace' } }],
      getConfiguration() { return { get(key, fallback) { return conf[key] ?? fallback; } }; },
      onDidChangeConfiguration(fn) { changed = fn; return disposable(); },
      onDidChangeWorkspaceFolders(fn) { foldersChanged = fn; return disposable(); },
    },
  };
  const context = { subscriptions: [] };
  const sidebar = createSidebar(vscode, context, map, invoke);
  return { sidebar, commands, calls, providers, vscode, conf, context, changed: () => changed({ affectsConfiguration: () => true }), foldersChanged: () => foldersChanged() };
}
function findingFile(t, extra = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'erf-sidebar-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const file = path.join(dir, 'finding.json');
  fs.writeFileSync(file, JSON.stringify({ apiVersion: 'rulefactory.io/v1', kind: 'FindingView', findingId: 'f', ruleId: 'r', fingerprint: 'fp', message: 'safe', location: { path: 'src/Main.java', line: 1, column: 1 }, ...extra }));
  return file;
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

test('finding privacy rejection prevents invocation; exception scope is current validated file', async t => {
  let calls = 0;
  const rejected = host(async () => { calls++; }, { file: findingFile(t, { sourceCode: 'private' }) });
  await rejected.sidebar.open('erf.explainFinding'); await rejected.sidebar.message({ type: 'pickFile' }); await rejected.sidebar.execute({});
  assert.equal(calls, 0); assert.equal(rejected.sidebar.state.phase, 'failed');
  let args;
  const h = host(async a => { args = a; return { code: 0, stdout: '{}', stderr: '' }; }, { file: findingFile(t) });
  await h.sidebar.open('erf.requestException'); await h.sidebar.message({ type: 'pickFile' });
  await h.sidebar.execute({ actor: 'confirmed-user', reason: 'reason with spaces' });
  assert.equal(h.sidebar.state.phase, 'completed');
  assert.deepEqual(args.slice(3), ['--scope', 'src/Main.java', '--reason', 'reason with spaces', '--actor', 'confirmed-user']);
});

test('missing endpoint, CLI refusal, unavailable binary and receipt guards are visible', async () => {
  const missing = host(); await missing.sidebar.open('erf.submissionStatus'); assert.equal(missing.sidebar.state.phase, 'unconfigured');
  for (const [result, phase] of [
    [{ code: 3, stdout: '', stderr: 'pending' }, 'failed'],
    [{ code: -1, reason: 'UNAVAILABLE', stderr: 'ENOENT' }, 'unavailable'],
    [{ code: 0, stdout: '{"status":"RECEIVED_ONLY","gateMutated":true}' }, 'failed'],
    [{ code: 0, stdout: 'invalid-json' }, 'failed'],
    [{ code: 0, stdout: '{"status":"RECEIVED_ONLY","exceptionApproved":false,"gateMutated":false,"scmPublished":false}' }, 'completed'],
  ]) {
    const h = host(async () => result, { config: { serverEndpoint: 'http://localhost:1' } });
    await h.sidebar.open('erf.submissionStatus'); await h.sidebar.execute({ requestId: 'sha256:x' });
    assert.equal(h.sidebar.state.phase, phase);
  }
});

test('running cancels, prevents duplicate invocation and discards stale configuration result', async () => {
  let release; let calls = 0; let signal;
  const h = host((a,c,r,s) => { calls++; signal=s; return new Promise(resolve => { release=resolve; }); }, { config: { serverEndpoint: 'http://localhost:1' } });
  await h.sidebar.open('erf.submissionStatus'); const run = h.sidebar.execute({ requestId: 'r' });
  assert.equal(h.sidebar.state.phase, 'running'); await h.sidebar.execute({ requestId: 'second' }); assert.equal(calls, 1);
  await h.sidebar.message({ type: 'cancel' }); assert.equal(signal.aborted, true);
  release({ code: 0, stdout: '{}' }); await run; assert.equal(h.sidebar.state.phase, 'cancelled');
  const stale = h.sidebar.execute({ requestId: 'r' }); h.conf.serverEndpoint = ''; h.changed();
  release({ code: 0, stdout: '{"status":"RECEIVED_ONLY"}' }); await stale;
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
  fs.mkdirSync(path.join(root, 'src')); fs.writeFileSync(path.join(root, 'src/Main.java'), 'class Main {}');
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
