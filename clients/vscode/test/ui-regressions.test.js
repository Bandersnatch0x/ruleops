'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const { host, findingFile, attachWebview } = require('./sidebar-host');
const { runRf, processTimeoutMs } = require('../src/rf-invoker');
const { SCENARIOS, validateMessage } = require('../src/sidebar');
const { webviewMain } = require('../src/sidebar-webview');
const manifest = require('../package.json');
const map = require('../../ide-command-map.json');

// A small DOM seam executes the actual webview script without adding a DOM framework.
function clientDom(webview) {
  const nodes = [];
  class Element {
    constructor(tag) { this.tagName = tag.toUpperCase(); this.children = []; this.dataset = {}; this.listeners = {}; this.value = ''; this.textContent = ''; this.hidden = false; nodes.push(this); }
    append(...children) { this.children.push(...children); }
    replaceChildren(...children) { this.children = children; if (this.tagName === 'SELECT') this.value = children[0]?.value || ''; }
    setAttribute(name, value) { this[name] = value; }
    addEventListener(name, fn) { this.listeners[name] = fn; }
    setCustomValidity(message) { this.validationMessage = message; }
    reportValidity() { this.checked = true; return !nodes.some(node => node.validationMessage && !node.disabled); }
    get selectedOptions() { return this.children.filter(node => node.value === this.value); }
    focus() { document.activeElement = this; }
  }
  for (const [, tag, attrs] of webview.html.matchAll(/<(\w+)([^>]*\bid="[^"]+"[^>]*)>/g)) {
    const element = new Element(tag); element.id = attrs.match(/\bid="([^"]+)"/)[1];
  }
  for (const [, action] of webview.html.matchAll(/data-action="([^"]+)"/g)) { const element = new Element('button'); element.dataset.action = action; }
  const document = { activeElement: null, listeners: {}, createElement: tag => new Element(tag), getElementById: id => nodes.find(node => node.id === id),
    querySelectorAll: () => nodes.filter(node => node.dataset.action), addEventListener(name, fn) { this.listeners[name] = fn; } };
  const window = { addEventListener(name, fn) { this[name] = fn; } };
  const sent = [];
  vm.runInNewContext(`(${webviewMain.toString()})();`, { document, window, acquireVsCodeApi: () => ({ postMessage: data => sent.push(JSON.parse(JSON.stringify(data))) }) });
  return { document, sent, get: document.getElementById, update: data => window.message({ data }),
    event(type, target, extra = {}) { const event = { target, preventDefault() { this.prevented = true; }, ...extra }; target.listeners[type](event); return event; } };
}

test('local execution deadlines are separate from HTTP timeouts for every long-running argv', () => {
  const config = { requestTimeoutMs: 5000 };
  for (const args of [['scan'], ['doctor'], ['config', 'show'], ['config', 'validate'], ['standard', 'validate'], ['rule', 'validate']]) {
    assert.equal(processTimeoutMs(args, config), 600000);
  }
  assert.equal(processTimeoutMs(['scan', '--deadline-ms', '900000'], config), 900000);
  for (const args of [['feedback', 'submit'], ['submission', 'status'], ['registry', 'sync']]) assert.equal(processTimeoutMs(args, config), 5000);
  for (const deadline of ['', '0', '-1', 'NaN', '3.5', '2147483648', undefined]) {
    assert.throws(() => processTimeoutMs(['scan', '--deadline-ms', deadline], config), /INVALID_PROCESS_TIMEOUT/);
  }
});

test('real local scan survives a short HTTP timeout; explicit deadline still stops it', { skip: process.platform === 'win32', timeout: 5000 }, async t => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'erf-timeout-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const launcher = path.join(dir, 'rf');
  fs.writeFileSync(launcher, '#!/bin/sh\nsleep 0.15\nprintf done\n', { mode: 0o755 });
  const config = { rfPath: launcher, requestTimeoutMs: 20 };
  const result = await runRf(['scan'], config, dir);
  assert.equal(result.code, 0); assert.equal(result.stdout, 'done'); assert.equal(result.reason, undefined);
  assert.equal((await runRf(['scan', '--deadline-ms', '20'], config, dir)).reason, 'TIMEOUT');
});

test('stderr callback streams before exit, preserves split UTF-8/CRLF and flushes final partial line', { timeout: 5000 }, async t => {
  const lines = [];
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'erf-stream-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const ack = path.join(dir, 'progress-seen');
  // The child cannot finish until the parent receives its first progress line.
  const script = "const fs=require('node:fs');const b=Buffer.from('扫描\\r\\n');process.stderr.write(b.subarray(0,1));setTimeout(()=>process.stderr.write(b.subarray(1)),30);const t=setInterval(()=>{if(fs.existsSync(process.argv[1])){clearInterval(t);process.stderr.write('tail');process.stdout.write('receipt')}},20)";
  const result = await runRf(['-e', script, ack], { rfPath: process.execPath, requestTimeoutMs: 3000 }, process.cwd(), undefined, {
    onStderrLine(line) { lines.push(line); if (line === '扫描') fs.writeFileSync(ack, 'seen'); },
  });
  assert.equal(result.reason, undefined); assert.equal(result.code, 0);
  assert.deepEqual(lines, ['扫描', 'tail']);
  assert.equal(result.stderr, '扫描\r\ntail'); assert.equal(result.stdout, 'receipt');
});

test('run boundary rejects missing and whitespace-only actor/reason; edits remain legal', async t => {
  const file = findingFile(t); let invocations = 0;
  const h = host(async () => { invocations++; }, { file });
  for (const id of ['erf.draftFeedback', 'erf.requestException']) {
    const scene = SCENARIOS.find(scene => scene.id === id);
    assert.doesNotThrow(() => validateMessage({ type: 'edit', fields: { actor: '', reason: '' } }, scene));
    for (const fields of [{}, { actor: 'user', reason: '  ' }, { actor: '\t', reason: 'why' }]) {
      assert.throws(() => validateMessage({ type: 'run', fields }, scene), /MISSING_ARG/);
      await h.sidebar.open(id); await h.sidebar.message({ type: 'run', fields });
      assert.equal(h.sidebar.state.phase, 'failed');
    }
  }
  assert.equal(invocations, 0); assert.ok(!h.calls.some(call => call[0] === 'confirm'));
});

test('live progress uses messages, stable skeleton, immutable clickable receipt history and ThemeIcons', async () => {
  let release; let started;
  const ready = new Promise(resolve => { started = resolve; });
  const h = host((args, config, root, signal, options) => new Promise(resolve => {
    release = resolve; options.onStderrLine('stage=resolve running'); options.onStderrLine('stage=scan running'); started();
  }));
  await h.sidebar.open('erf.doctor'); const webview = attachWebview(h);
  const running = h.sidebar.execute({ format: 'text' }); await ready;
  assert.equal(webview.update.busy, true); assert.equal(webview.update.progress, 'stage=resolve running\nstage=scan running'); assert.equal(webview.writes, 1);
  release({ code: 0, stdout: 'receipt A', stderr: 'stage=resolve running' }); await running;
  const node = h.providers.tree.getChildren({ id: 'runs' })[0];
  const item = h.providers.tree.getTreeItem(node);
  assert.equal(item.command.command, 'erf.showRun'); assert.equal(item.iconPath.id, 'history'); assert.ok(!Number.isNaN(Date.parse(item.description)));
  assert.ok(h.providers.tree.getTreeItem(SCENARIOS[0]).iconPath.id);
  await h.sidebar.open('erf.configShow');
  assert.equal(webview.update.progress, '');
  await h.commands.get(item.command.command)(...item.command.arguments);
  assert.match(webview.update.detail, /receipt A/); assert.ok(webview.update.detail.includes(item.description));
  assert.equal(webview.writes, 1);
  await h.sidebar.message({ type: 'ready' }); assert.match(webview.update.detail, /receipt A/);
  assert.ok(Object.isFrozen(node.run));
});

test('actual DOM update preserves input/caret and live region, validates required text and supports Escape', async t => {
  const h = host(undefined, { file: findingFile(t) });
  await h.sidebar.open('erf.draftFeedback'); await h.sidebar.message({ type: 'pickFile' });
  const webview = attachWebview(h); const dom = clientDom(webview); dom.update(webview.update);
  assert.equal(dom.sent[0].type, 'ready');
  const reason = dom.get('field-reason'); const actor = dom.get('field-actor'); const status = dom.get('status');
  assert.equal(reason.required, true); assert.equal(actor.required, true);
  reason.value = 'in-flight <script>text</script>'; reason.selectionStart = 8; reason.focus();
  dom.update({ ...webview.update, status: '运行中', busy: true, progress: 'scan started' });
  assert.equal(dom.get('field-reason'), reason); assert.equal(dom.document.activeElement, reason); assert.equal(reason.selectionStart, 8);
  assert.equal(reason.value, 'in-flight <script>text</script>'); assert.equal(dom.get('status'), status); assert.equal(status.textContent, '运行中');
  assert.equal(dom.get('progress').hidden, false);
  dom.get('execute').focus();
  dom.update({ ...webview.update, status: '运行中', busy: true });
  assert.equal(dom.document.activeElement, dom.get('cancel'));
  assert.equal(dom.get('execute-label').textContent, webview.update.executeLabel);
  const runningChoice = dom.get('field-classification'); runningChoice.value = 'documentation';
  dom.event('input', dom.get('fields'), { target: runningChoice });
  assert.equal(runningChoice.value, webview.update.controls.find(field => field.name === 'classification').value);
  const escape = { key: 'Escape', preventDefault() { this.prevented = true; } }; dom.document.listeners.keydown(escape);
  assert.equal(dom.sent.at(-1).type, 'cancel'); assert.equal(escape.prevented, true);
  dom.update(webview.update);
  const classification = dom.get('field-classification'); classification.value = 'documentation'; classification.focus();
  const changedChoices = webview.update.controls.map(field => field.name !== 'classification' ? field : { ...field, choices: field.choices.map(choice => ({ ...choice, label: choice.label + ' updated' })) });
  dom.update({ ...webview.update, controls: changedChoices });
  assert.equal(dom.get('field-classification'), classification); assert.equal(classification.value, 'documentation');
  actor.value = '   '; reason.value = 'reason';
  const form = dom.get('fields'); dom.event('submit', form);
  assert.equal(form.checked, true); assert.notEqual(dom.sent.at(-1).type, 'run'); assert.ok(actor.validationMessage);
  actor.value = 'confirmed'; dom.event('input', form); dom.event('submit', form);
  assert.equal(dom.sent.at(-1).type, 'run'); assert.equal(dom.sent.at(-1).fields.actor, 'confirmed');
  dom.update({ ...webview.update, detail: '<img src=x onerror=evil()>' });
  assert.equal(dom.get('output').textContent, '<img src=x onerror=evil()>');
});

test('webview has independent scroll panes, theme buttons/editor font, progress and keyboard output', () => {
  const webview = attachWebview(host());
  for (const token of ['button-foreground', 'button-background', 'button-hoverBackground', 'button-secondaryForeground', 'button-secondaryBackground', 'disabledForeground', 'contrastBorder', 'editor-font-family', 'progressBar-background']) assert.ok(webview.html.includes('--vscode-' + token));
  assert.match(webview.html, /id="output" tabindex="0" role="region"/);
  assert.match(webview.html, /grid-template-rows:minmax\(0,1fr\) auto minmax\(0,1fr\)/);
  assert.match(webview.html, /#form-pane,#output\{min-height:0;overflow:auto\}/);
  assert.match(webview.html, /prefers-reduced-motion/); assert.doesNotMatch(webview.html, /opacity:\.5|\.innerHTML/);
  assert.ok(webview.html.indexOf('id="fields"') < webview.html.indexOf('id="actions"'));
  assert.ok(webview.html.indexOf('id="actions"') < webview.html.indexOf('id="output"'));
});

test('all scenarios have command palette entries/icons and mapped inputs use shared labels', async () => {
  const entries = manifest.contributes.commands;
  const h = host(); const webview = attachWebview(h);
  for (const scene of SCENARIOS) {
    const entry = entries.filter(entry => entry.command === scene.id);
    assert.equal(entry.length, 1, scene.id); assert.ok(entry[0].icon);
    await h.sidebar.open(scene.id);
    const params = map.commands.find(command => command.commandId === scene.cli)?.params || [];
    for (const field of webview.update.controls) {
      const param = params.find(param => param.name === field.name);
      if (field.name === 'deadlineMs') { assert.match(field.label, /600 秒/); assert.doesNotMatch(field.label, /不截断/); }
      else if (param?.label) assert.equal(field.label, param.label);
    }
  }
  for (const [id, label] of [['erf.showFinding', '展示 Finding'], ['erf.navigateToLocation', '导航到位置'], ['erf.draftFeedback', '起草反馈'], ['erf.requestException', '申请例外'], ['erf.registryShow', '核对独立发布 pin']]) assert.equal(SCENARIOS.find(scene => scene.id === id).label, label);
});
