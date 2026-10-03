'use strict';
const fs = require('node:fs/promises');
const { randomBytes } = require('node:crypto');
const core = require('./core');
const { resolveSafeWorkspaceFile } = require('./path-safety');
const { runRf } = require('./rf-invoker');

const SCENARIOS = [
  { id: 'erf.showFinding', label: '显示 Finding', section: 'scan_findings', fields: [] },
  { id: 'erf.navigateToLocation', label: '跳转到 Finding', section: 'scan_findings', fields: [] },
  { id: 'erf.explainFinding', cli: 'rf.explain', label: '解释 Finding', section: 'scan_findings', fields: [] },
  { id: 'erf.draftFeedback', cli: 'rf.feedback.draft', label: '起草反馈', section: 'feedback_exceptions', fields: ['classification', 'reason', 'actor'] },
  { id: 'erf.requestException', cli: 'rf.exception.request', label: '申请例外（当前文件）', section: 'feedback_exceptions', fields: ['reason', 'actor'] },
  { id: 'erf.submitFeedback', cli: 'rf.feedback.submit', label: '提交反馈', section: 'feedback_exceptions', fields: [], server: true },
  { id: 'erf.submitException', cli: 'rf.exception.submit', label: '提交例外', section: 'feedback_exceptions', fields: [], server: true },
  { id: 'erf.submissionStatus', cli: 'rf.submission.status', label: '查询提交状态', section: 'feedback_exceptions', fields: ['requestId', 'artifactDigest'], server: true },
];
const LABELS = { ready: '就绪', unconfigured: '未配置', unavailable: '不可用', running: '运行中', failed: '失败', cancelled: '已取消', completed: '已完成（非批准）' };
const escape = (value) => String(value ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

function validateMessage(message, scene) {
  if (!message || typeof message !== 'object' || Array.isArray(message)) throw new Error('INVALID_MESSAGE');
  const keys = Object.keys(message);
  if (['pickFile', 'workspace', 'cancel', 'refresh', 'settings'].includes(message.type) && keys.length === 1) return message;
  if (!['run', 'edit'].includes(message.type) || keys.length !== 2 || !keys.includes('fields') || !scene) throw new Error('INVALID_MESSAGE');
  const fields = message.fields;
  if (!fields || typeof fields !== 'object' || Array.isArray(fields)) throw new Error('INVALID_FIELDS');
  for (const [key, value] of Object.entries(fields)) {
    if (!scene.fields.includes(key) || typeof value !== 'string' || value.length > 16384 || value.includes('\0')) throw new Error('INVALID_FIELD:' + key);
  }
  return message;
}

function createSidebar(vscode, context, commandMap, invoke = runRf) {
  const events = new vscode.EventEmitter();
  let scene = SCENARIOS[0];
  let root = vscode.workspace.workspaceFolders?.length === 1 ? vscode.workspace.workspaceFolders[0].uri.fsPath : null;
  let file = null;
  let fields = {};
  let view;
  let controller;
  let disposed = false;
  let generation = 0;
  let state = { phase: 'ready', detail: '' };
  const history = [];
  const config = () => {
    const c = vscode.workspace.getConfiguration('erf', root ? vscode.Uri.file(root) : undefined);
    return { rfPath: c.get('rfPath', 'rf'), serverEndpoint: c.get('serverEndpoint', ''), requestTimeoutMs: c.get('requestTimeoutMs', 5000) };
  };
  const publish = () => {
    if (disposed) return;
    events.fire();
    void vscode.commands.executeCommand('setContext', 'erf.running', !!controller);
    if (view) view.webview.html = html();
  };
  const setState = (phase, detail = '') => { state = { phase, detail }; publish(); };
  function refresh() {
    if (controller) return;
    const c = config();
    if (!root) return setState('unconfigured', '请选择工作区；不会默认取多根工作区的第一项。');
    if (!scene.fields) return setState('unavailable', '阶段 1 尚未提供此命令的参数面板；不执行模板或自动准备安装资产。\n' + scene.commandTemplate);
    if (!c.rfPath || (scene.server && !c.serverEndpoint)) return setState('unconfigured', '请在 ERF 设置中显式配置 rf 路径 / Server endpoint。');
    if (!Number.isInteger(c.requestTimeoutMs) || c.requestTimeoutMs <= 0) return setState('unconfigured', 'requestTimeoutMs 必须为正整数。');
    setState('ready', 'rf 可用性将在显式执行时确认。提交不等于批准 / Gate PASS / SCM 写入。');
  }
  const tree = vscode.window.createTreeView('erf.scenarios', { treeDataProvider: {
    onDidChangeTreeData: events.event,
    getChildren(node) {
      if (!node) return [...Object.entries(commandMap.sections).map(([id, label]) => ({ id, label, group: true })), { id: 'runs', label: '运行记录', group: true }];
      if (node.id === 'runs') return history.map((run, i) => ({ id: 'run-' + i, label: run }));
      return [...SCENARIOS.filter(s => s.section === node.id), ...commandMap.commands.filter(c => c.section === node.id && !SCENARIOS.some(s => s.cli === c.commandId)).map(c => ({ ...c, id: c.commandId, label: c.commandTemplate, unavailable: true }))];
    },
    getTreeItem(node) {
      const collapsed = ['pilot_governance', 'operations_status', 'runs'].includes(node.id);
      const item = new vscode.TreeItem(node.label, node.group ? (collapsed ? 1 : 2) : 0);
      item.id = node.id;
      if (node.section) {
        item.command = { command: 'erf.selectScene', title: node.label, arguments: [node.id] };
        item.description = node.unavailable ? '不可用 · 待后续阶段' : scene.id === node.id ? LABELS[state.phase] : '';
      }
      return item;
    },
    getParent(node) { return node.section ? { id: node.section, label: commandMap.sections[node.section], group: true } : undefined; },
  } });
  async function open(id) {
    if (controller) { await vscode.commands.executeCommand('erf.details.focus'); return; }
    const next = SCENARIOS.find(s => s.id === id) || commandMap.commands.find(c => c.commandId === id);
    if (!next) throw new Error('UNKNOWN_SCENE');
    const previous = scene;
    scene = next.id ? next : { ...next, id: next.commandId, label: next.commandTemplate };
    fields = {};
    // A request artifact must never silently become a Finding input (or vice versa).
    if (Boolean(previous.server) !== Boolean(scene.server)) file = null;
    generation++;
    refresh();
    await vscode.commands.executeCommand('workbench.view.extension.erf');
    await tree.reveal(scene, { select: true, focus: false });
    await vscode.commands.executeCommand('erf.details.focus');
  }
  async function execute(input) {
    if (controller) return;
    validateMessage({ type: 'run', fields: input }, scene);
    refresh();
    if (state.phase !== 'ready') return;
    const selected = scene;
    const c = config();
    const runRoot = root;
    fields = { ...input };
    const abort = new AbortController();
    controller = abort;
    const version = generation;
    setState('running', selected.label);
    try {
      let finding;
      if (!selected.server) {
        if (!file) throw new Error('请先选择 FindingView JSON');
        finding = JSON.parse(await fs.readFile(file, 'utf8'));
        const check = core.validateFindingView(finding);
        if (!check.ok) throw new Error('FindingView 被拒绝：' + check.reason);
      }
      if (abort.signal.aborted || generation !== version || disposed) return;
      let args;
      switch (selected.id) {
        case 'erf.showFinding': setState('completed', JSON.stringify(finding, null, 2)); return;
        case 'erf.navigateToLocation': {
          const target = resolveSafeWorkspaceFile(runRoot, finding.location.path);
          const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(target));
          if (abort.signal.aborted || generation !== version) return;
          const editor = await vscode.window.showTextDocument(doc);
          if (abort.signal.aborted || generation !== version || disposed) return;
          const pos = new vscode.Position(Math.max(0, finding.location.line - 1), Math.max(0, finding.location.column - 1));
          editor.selection = new vscode.Selection(pos, pos);
          editor.revealRange(new vscode.Range(pos, pos));
          setState('completed', finding.location.path); return;
        }
        case 'erf.explainFinding': args = core.buildExplainArgs(file, { format: 'text' }); break;
        case 'erf.draftFeedback': args = core.buildFeedbackDraftArgs(file, input); break;
        case 'erf.requestException': args = core.buildExceptionRequestArgs(file, { ...input, scope: finding.location.path }); break;
        case 'erf.submitFeedback': case 'erf.submitException':
          if (!file) throw new Error('请先选择已保存请求 JSON');
          args = core.buildSubmitArgs(selected.id === 'erf.submitFeedback' ? 'feedback' : 'exception', file, { server: c.serverEndpoint, timeoutMs: c.requestTimeoutMs }); break;
        case 'erf.submissionStatus': args = core.buildSubmissionStatusArgs({ ...input, server: c.serverEndpoint, timeoutMs: c.requestTimeoutMs }); break;
        default: throw new Error('UNAVAILABLE_SCENE');
      }
      if (abort.signal.aborted) return;
      const result = await invoke(args, c, runRoot, abort.signal);
      if (generation !== version || disposed) return;
      if (result.reason === 'CANCELLED' || abort.signal.aborted) return setState('cancelled', '已停止；远端请求可能已经接收，取消不代表回滚，请查询状态。');
      if (result.reason === 'UNAVAILABLE') return setState('unavailable', result.stderr);
      if (result.reason || core.classifyExitCode(result.code) !== 'OK') return setState('failed', `${result.reason || core.classifyExitCode(result.code)} · exit=${result.code}\n${result.stderr || result.stdout}`);
      let detail = result.stdout;
      if (selected.server) {
        const receipt = JSON.parse(result.stdout);
        if (!receipt || typeof receipt !== 'object' || Array.isArray(receipt)) throw new Error('INVALID_RECEIPT');
        const rendered = core.renderReceiptGuards(receipt);
        if (!rendered.guardsHold) throw new Error('回执违反权威守卫：' + rendered.violations.join(','));
        detail = rendered.status + '\n' + rendered.note + '\n' + result.stdout;
      }
      if (selected.id === 'erf.draftFeedback') detail = core.statusLabel('PROPOSED') + '\n' + detail;
      if (selected.id === 'erf.requestException') detail = core.statusLabel('REQUESTED') + '\n' + detail;
      setState('completed', 'exit=0\n' + detail);
    } catch (error) {
      if (generation === version && !disposed) setState(abort.signal.aborted ? 'cancelled' : 'failed', error.message);
    } finally {
      controller = null;
      if (generation === version && !disposed) {
        if (abort.signal.aborted) state = { phase: 'cancelled', detail: '已取消；远端副作用未知，必要时查询回执。' };
        history.unshift(selected.label + ' · ' + LABELS[state.phase]);
        history.splice(20);
        publish();
      } else refresh();
    }
  }
  async function message(raw) {
    try {
      const m = validateMessage(raw, scene);
      if (m.type === 'cancel') { controller?.abort(); return; }
      if (controller) return;
      if (m.type === 'edit') { fields = { ...m.fields }; return; }
      if (m.type === 'run') return await execute(m.fields);
      if (m.type === 'refresh') return refresh();
      if (m.type === 'settings') return await vscode.commands.executeCommand('workbench.action.openSettings', 'erf');
      const version = generation;
      if (m.type === 'pickFile') {
        const picked = await vscode.window.showOpenDialog({ canSelectMany: false, filters: { JSON: ['json'] }, defaultUri: root ? vscode.Uri.file(root) : undefined });
        if (picked?.length && generation === version) { file = picked[0].fsPath; publish(); }
      } else if (m.type === 'workspace') {
        const picked = await vscode.window.showWorkspaceFolderPick();
        if (picked && generation === version) { root = picked.uri.fsPath; file = null; fields = {}; generation++; refresh(); }
      }
    } catch (error) { if (!controller) setState('failed', error.message); }
  }
  function html() {
    const nonce = randomBytes(18).toString('base64');
    const busy = !!controller;
    const disabled = busy ? 'disabled' : '';
    const controls = (scene.fields || []).map(name => `<label>${escape({ reason: '理由', actor: '提交者（本人确认）', classification: '分类', requestId: 'requestId', artifactDigest: 'artifactDigest（requestId 留空时使用）' }[name])}${name === 'classification' ? `<select name="classification" ${disabled}>${['true-fp', 'not-fp', 'scope-issue', 'documentation'].map(v => `<option ${fields[name] === v ? 'selected' : ''}>${v}</option>`).join('')}</select>` : `<textarea name="${name}" ${disabled}>${escape(fields[name])}</textarea>`}</label>`).join('');
    return `<!doctype html><html lang="zh-CN"><head><meta charset="UTF-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';"><meta name="viewport" content="width=device-width,initial-scale=1"><style nonce="${nonce}">body{font-family:var(--vscode-font-family);color:var(--vscode-foreground);padding:12px}button,textarea,select{font:inherit;color:var(--vscode-input-foreground);background:var(--vscode-input-background);border:1px solid var(--vscode-input-border);padding:6px;margin:4px 0}button{cursor:pointer}button:disabled{opacity:.5;cursor:default}textarea,select{box-sizing:border-box;width:100%}label{display:block;margin-top:12px}pre,p{white-space:pre-wrap;overflow-wrap:anywhere}h2{font-size:1.1em}:focus-visible{outline:2px solid var(--vscode-focusBorder)}</style></head><body><button data-action="workspace" ${disabled}>选择工作区</button><p>${escape(root || '未选择工作区')}<br>Server：${config().serverEndpoint ? '已配置（连接由 CLI 校验）' : '未配置'}</p><h2>${escape(scene.label)}</h2><p role="status">${LABELS[state.phase]}</p>${scene.fields && scene.id !== 'erf.submissionStatus' ? `<button data-action="pickFile" ${disabled}>${scene.server ? '选择已保存请求 JSON' : '选择 FindingView JSON'}</button><p>${escape(file || '未选择文件')}</p>` : ''}${scene.id === 'erf.requestException' ? '<p>scope 使用执行时验证的 Finding 当前文件；此阶段不扩范围。</p>' : ''}<form>${controls}<button type="submit" ${busy || !['ready', 'failed', 'completed', 'cancelled'].includes(state.phase) ? 'disabled' : ''}>执行所选场景</button></form><button data-action="cancel" ${busy ? '' : 'disabled'}>取消</button><button data-action="refresh" ${disabled}>刷新</button><button data-action="settings" ${disabled}>设置</button><p>提交 ≠ 批准 ≠ Gate PASS；不写入 SCM。</p><pre aria-live="polite">${escape(state.detail)}</pre><script nonce="${nonce}">const api=acquireVsCodeApi();document.querySelectorAll('[data-action]').forEach(b=>b.addEventListener('click',()=>api.postMessage({type:b.dataset.action})));document.querySelector('form').addEventListener('input',e=>api.postMessage({type:'edit',fields:Object.fromEntries(new FormData(e.currentTarget))}));document.querySelector('form').addEventListener('submit',e=>{e.preventDefault();api.postMessage({type:'run',fields:Object.fromEntries(new FormData(e.target))});});</script></body></html>`;
  }
  const register = (id, fn) => context.subscriptions.push(vscode.commands.registerCommand(id, fn));
  register('erf.selectScene', open);
  register('erf.refresh', refresh);
  register('erf.cancel', () => controller?.abort());
  register('erf.settings', () => vscode.commands.executeCommand('workbench.action.openSettings', 'erf'));
  context.subscriptions.push(tree, events, vscode.window.registerWebviewViewProvider('erf.details', {
    resolveWebviewView(resolved) {
      view = resolved;
      view.webview.options = { enableScripts: true, localResourceRoots: [] };
      context.subscriptions.push(view.webview.onDidReceiveMessage(message), view.onDidDispose(() => { view = undefined; }));
      publish();
    },
  }), vscode.workspace.onDidChangeConfiguration(event => {
    if (!event.affectsConfiguration('erf')) return;
    generation++; controller?.abort(); fields = {}; refresh();
  }), vscode.workspace.onDidChangeWorkspaceFolders(() => {
    if (root && !vscode.workspace.workspaceFolders?.some(f => f.uri.fsPath === root)) {
      generation++; controller?.abort(); root = null; file = null; fields = {}; refresh();
    }
  }), { dispose() { disposed = true; generation++; controller?.abort(); } });
  refresh();
  return { open, execute, message, get state() { return state; } };
}
module.exports = { createSidebar, SCENARIOS, validateMessage };
