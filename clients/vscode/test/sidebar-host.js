'use strict';
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createSidebar } = require('../src/sidebar');
const { runRf } = require('../src/rf-invoker');
const map = require('../../ide-command-map.json');
function host(invoke, options = {}) {
  const commands = new Map(); const calls = []; const providers = {};
  const conf = { rfPath: 'rf', serverEndpoint: '', requestTimeoutMs: 5000, ...options.config };
  let changed; let foldersChanged;
  const disposable = () => ({ dispose() {} });
  const vscode = {
    EventEmitter: class { constructor() { this.event = () => disposable(); } fire() {} dispose() {} },
    ThemeIcon: class { constructor(id) { this.id = id; } },
    TreeItem: class { constructor(label, state) { this.label = label; this.collapsibleState = state; } },
    Uri: { file: fsPath => ({ fsPath }) },
    commands: { registerCommand(id, fn) { commands.set(id, fn); return disposable(); }, async executeCommand(...args) { calls.push(args); } },
    window: {
      createTreeView(id, opts) { providers.tree = opts.treeDataProvider; return { ...disposable(), async reveal(node) { calls.push(['reveal', node.id]); } }; },
      registerWebviewViewProvider(id, provider) { providers.webview = provider; return disposable(); },
      async showOpenDialog() { return options.file ? [{ fsPath: options.file }] : undefined; },
      async showSaveDialog() { return options.cancelSave ? undefined : { fsPath: options.output || path.join(options.root || path.dirname(options.file), 'saved-request.json') }; },
      async showWarningMessage(message, settings, action) { calls.push(['confirm', message]); return options.cancelConfirm ? undefined : action; },
      async showWorkspaceFolderPick() { return { uri: { fsPath: options.nextRoot || '/chosen' } }; },
    },
    workspace: {
      workspaceFolders: options.folders || [{ uri: { fsPath: options.root || (options.file ? path.dirname(options.file) : '/workspace') } }],
      getConfiguration() { return { get(key, fallback) { return conf[key] ?? fallback; } }; },
      onDidChangeConfiguration(fn) { changed = fn; return disposable(); },
      onDidChangeWorkspaceFolders(fn) { foldersChanged = fn; return disposable(); },
    },
  };
  const storage = { 'erf.requests': options.records || [] };
  const context = { subscriptions: [], workspaceState: { get(key, fallback) { return storage[key] ?? fallback; }, async update(key, value) { if (options.indexFailure) throw new Error('INDEX_WRITE_FAILED'); storage[key] = value; } } };
  const wrapped = async (...args) => args[0][0] === 'config' && !options.realConfig
    ? (options.configResult || { code: 0, stdout: JSON.stringify({ config: options.effective || { modules: [], source_roots: [] } }), stderr: '' })
    : (invoke || runRf)(...args);
  const sidebar = createSidebar(vscode, context, map, wrapped);
  return { sidebar, commands, calls, providers, vscode, conf, context, changed: () => changed({ affectsConfiguration: () => true }), foldersChanged: () => foldersChanged() };
}
function findingFile(t, extra = {}, parent = os.tmpdir()) {
  const dir = fs.mkdtempSync(path.join(parent, 'erf-sidebar-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  fs.mkdirSync(path.join(dir, 'src')); fs.writeFileSync(path.join(dir, 'src/Main.java'), 'class Main {}');
  const file = path.join(dir, 'finding.json');
  fs.writeFileSync(file, JSON.stringify({ apiVersion: 'rulefactory.io/v1', kind: 'FindingView', findingId: 'f', ruleId: 'r', fingerprint: 'fp', message: 'safe', location: { path: 'src/Main.java', line: 1, column: 1 }, ...extra }));
  return file;
}



function attachWebview(h) {
  const messages = [];
  const webview = { writes: 0, messages, onDidReceiveMessage(fn) { this.message = fn; return { dispose() {} }; },
    postMessage(data) { messages.push(data); return Promise.resolve(true); },
    set html(value) { this.source = value; this.writes++; }, get html() { return this.source; },
    get update() { return messages.at(-1); } };
  h.providers.webview.resolveWebviewView({ webview, onDidDispose() { return { dispose() {} }; } });
  return webview;
}
module.exports = { host, findingFile, attachWebview };
