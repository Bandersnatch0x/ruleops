'use strict';
// VS Code 绑定层（薄）：把 core.js 的纯逻辑接到编辑器命令上，并通过 rf 子进程复用
// Runner/Thin Client seam。本层刻意保持极薄——所有隐私/路径安全/退出码/requestId
// 语义都由 core.js 与 rf 承担，插件不复制任何 Runtime/Factory/Server/Gate 逻辑。
// 真实 IDE 交互闭环（导航→解释→草稿→可选提交）的验收需在真实 VS Code 版本上完成。

const vscode = require('vscode');
const fs = require('fs');
const { spawn } = require('child_process');
const core = require('./core');
const { resolveSafeWorkspaceFile } = require('./path-safety');

const MAX_RF_OUTPUT_BYTES = 1_048_576;

function cfg() {
  const c = vscode.workspace.getConfiguration('erf');
  return {
    rfPath: c.get('rfPath', 'rf'),
    serverEndpoint: c.get('serverEndpoint', ''),
    telemetryEnabled: c.get('telemetryEnabled', false),
    requestTimeoutMs: c.get('requestTimeoutMs', 5000),
  };
}

function workspaceRoot() {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) return null;
  return folders[0].uri.fsPath;
}

function runRf(args, cwd) {
  const conf = cfg();
  return new Promise((resolve) => {
    const child = spawn(conf.rfPath, args, {
      cwd: cwd || workspaceRoot() || process.cwd(),
      detached: process.platform !== 'win32',
    });
    let stdout = '';
    let stderr = '';
    let settled = false;
    const finish = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(result);
    };
    const kill = () => {
      try {
        if (process.platform !== 'win32' && child.pid) process.kill(-child.pid, 'SIGKILL');
        else child.kill('SIGKILL');
      } catch (_) {
        child.kill();
      }
    };
    const append = (kind, data) => {
      const text = data.toString();
      const next = kind === 'stdout' ? stdout + text : stderr + text;
      if (Buffer.byteLength(next, 'utf8') > MAX_RF_OUTPUT_BYTES) {
        kill();
        finish({ code: 3, stdout, stderr: stderr + '\nOUTPUT_LIMIT' });
        return;
      }
      if (kind === 'stdout') stdout = next;
      else stderr = next;
    };
    child.stdout.on('data', (d) => append('stdout', d));
    child.stderr.on('data', (d) => append('stderr', d));
    child.on('error', (err) => finish({ code: -1, stdout, stderr: String(err) }));
    child.on('close', (code) => finish({ code, stdout, stderr }));
    const timer = setTimeout(() => {
      kill();
      finish({ code: 3, stdout, stderr: stderr + '\nTIMEOUT' });
    }, Math.max(1, Number(conf.requestTimeoutMs) || 5000));
  });
}

async function pickFindingView() {
  const uris = await vscode.window.showOpenDialog({
    canSelectMany: false,
    filters: { 'FindingView JSON': ['json'] },
    title: '选择 FindingView JSON',
  });
  if (!uris || uris.length === 0) return null;
  const file = uris[0].fsPath;
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (e) {
    vscode.window.showErrorMessage('ERF: FindingView 解析失败：' + String(e));
    return null;
  }
  const check = core.validateFindingView(parsed);
  if (!check.ok) {
    vscode.window.showErrorMessage('ERF: FindingView 拒绝（fail closed）：' + check.reason);
    return null;
  }
  return { file, view: parsed };
}

async function cmdShowFinding() {
  const picked = await pickFindingView();
  if (!picked) return;
  const v = picked.view;
  vscode.window.showInformationMessage(
    `ERF Finding ${v.findingId} · rule=${v.ruleId} · ${v.location.path}:${v.location.line}:${v.location.column}`,
  );
}

async function cmdNavigate() {
  const picked = await pickFindingView();
  if (!picked) return;
  const root = workspaceRoot();
  if (!root) {
    vscode.window.showErrorMessage('ERF: 未打开工作区，无法导航。');
    return;
  }
  // 二次门禁：即便 FindingView 校验已过，导航前再次确认路径安全。
  const pathCheck = core.validateRelativePath(picked.view.location.path);
  if (!pathCheck.ok) {
    vscode.window.showErrorMessage('ERF: 位置越界，拒绝导航（fail closed）：' + pathCheck.reason);
    return;
  }
  let abs;
  try {
    abs = resolveSafeWorkspaceFile(root, picked.view.location.path);
  } catch (error) {
    vscode.window.showErrorMessage('ERF: 目标不是工作区内的真实文件，拒绝导航（fail closed）：' + String(error.message || error));
    return;
  }
  const doc = await vscode.workspace.openTextDocument(abs);
  const editor = await vscode.window.showTextDocument(doc);
  const line = Math.max(0, picked.view.location.line - 1);
  const col = Math.max(0, picked.view.location.column - 1);
  const pos = new vscode.Position(line, col);
  editor.selection = new vscode.Selection(pos, pos);
  editor.revealRange(new vscode.Range(pos, pos));
}

async function cmdExplain() {
  const picked = await pickFindingView();
  if (!picked) return;
  const res = await runRf(core.buildExplainArgs(picked.file, { format: 'text' }));
  const kind = core.classifyExitCode(res.code);
  if (kind === 'OK') {
    const parsed = core.parseExplainText(res.stdout);
    const chan = vscode.window.createOutputChannel('ERF Explain');
    chan.appendLine(res.stdout);
    chan.show(true);
    vscode.window.showInformationMessage(
      `ERF: rule=${parsed.rule_id || '?'} authority=${parsed.authority_status || '?'} authoritative=${parsed.authoritative || '?'}`,
    );
  } else if (kind === 'REFUSAL_OR_PENDING') {
    vscode.window.showWarningMessage('ERF: 解释被拒绝（元数据缺失/过期/撤销或摘要不匹配）。详见 stderr：' + res.stderr);
  } else {
    vscode.window.showErrorMessage('ERF: explain 失败（' + kind + '）：' + res.stderr);
  }
}

async function draftOrRequest(kind) {
  const picked = await pickFindingView();
  if (!picked) return;
  let opts;
  if (kind === 'feedback') {
    const classification = await vscode.window.showQuickPick(
      ['true-fp', 'not-fp', 'scope-issue', 'documentation'], { title: '反馈分类' });
    if (!classification) return;
    const reason = await vscode.window.showInputBox({ title: '反馈理由', ignoreFocusOut: true });
    if (!reason) return;
    const actor = await vscode.window.showInputBox({ title: 'actor（提交者标识）', ignoreFocusOut: true });
    if (!actor) return;
    opts = { classification, reason, actor };
  } else {
    const scope = await vscode.window.showInputBox({ title: '例外 scope（工作区相对 glob）', ignoreFocusOut: true });
    if (!scope) return;
    const reason = await vscode.window.showInputBox({ title: '例外理由', ignoreFocusOut: true });
    if (!reason) return;
    const actor = await vscode.window.showInputBox({ title: 'actor（申请者标识）', ignoreFocusOut: true });
    if (!actor) return;
    opts = { scope, reason, actor };
  }
  const args = kind === 'feedback'
    ? core.buildFeedbackDraftArgs(picked.file, opts)
    : core.buildExceptionRequestArgs(picked.file, opts);
  const res = await runRf(args);
  if (core.classifyExitCode(res.code) === 'OK') {
    const status = kind === 'feedback' ? 'PROPOSED' : 'REQUESTED';
    vscode.window.showInformationMessage('ERF: ' + core.statusLabel(status) + '\n' + res.stdout);
  } else {
    vscode.window.showErrorMessage('ERF: 生成失败：' + (res.stderr || res.stdout));
  }
}

async function submit(kind) {
  const conf = cfg();
  if (!conf.serverEndpoint) {
    vscode.window.showErrorMessage('ERF: 未配置 erf.serverEndpoint，提交动作已禁用（不自动发现）。');
    return;
  }
  const uris = await vscode.window.showOpenDialog({
    canSelectMany: false, filters: { 'Request JSON': ['json'] },
    title: kind === 'feedback' ? '选择 FeedbackDraft JSON' : '选择 ExceptionRequest JSON',
  });
  if (!uris || uris.length === 0) return;
  const args = core.buildSubmitArgs(kind, uris[0].fsPath,
    { server: conf.serverEndpoint, timeoutMs: conf.requestTimeoutMs });
  const res = await runRf(args);
  const cls = core.classifyExitCode(res.code);
  if (cls === 'OK') {
    let receipt = {};
    try { receipt = JSON.parse(res.stdout); } catch (e) { /* 文本回执 */ }
    const rendered = core.renderReceiptGuards(receipt);
    if (!rendered.guardsHold) {
      vscode.window.showErrorMessage('ERF: 回执违反权威守卫（' + rendered.violations.join(',') + '），fail closed。');
      return;
    }
    vscode.window.showInformationMessage('ERF: ' + rendered.status + '\n' + rendered.note);
  } else if (cls === 'REFUSAL_OR_PENDING') {
    vscode.window.showWarningMessage('ERF: 提交返回 PENDING/UNKNOWN，不作成功处理。');
  } else {
    vscode.window.showErrorMessage('ERF: 提交失败（' + cls + '）：' + res.stderr);
  }
}

async function cmdSubmissionStatus() {
  const conf = cfg();
  if (!conf.serverEndpoint) {
    vscode.window.showErrorMessage('ERF: 未配置 erf.serverEndpoint。');
    return;
  }
  const requestId = await vscode.window.showInputBox({ title: 'requestId（sha256:...）', ignoreFocusOut: true });
  if (!requestId) return;
  const args = core.buildSubmissionStatusArgs(
    { server: conf.serverEndpoint, requestId, timeoutMs: conf.requestTimeoutMs });
  const res = await runRf(args);
  vscode.window.showInformationMessage('ERF: submission-status（exit=' + res.code + '）\n' + (res.stdout || res.stderr));
}

function activate(context) {
  const reg = (id, fn) => context.subscriptions.push(vscode.commands.registerCommand(id, fn));
  reg('erf.showFinding', cmdShowFinding);
  reg('erf.navigateToLocation', cmdNavigate);
  reg('erf.explainFinding', cmdExplain);
  reg('erf.draftFeedback', () => draftOrRequest('feedback'));
  reg('erf.requestException', () => draftOrRequest('exception'));
  reg('erf.submitFeedback', () => submit('feedback'));
  reg('erf.submitException', () => submit('exception'));
  reg('erf.submissionStatus', cmdSubmissionStatus);
}

function deactivate() {}

module.exports = { activate, deactivate };
