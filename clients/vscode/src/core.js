'use strict';
// ERF Thin Client 纯逻辑核（零依赖，与 VS Code API 解耦，可离线单测）。
// 权威边界镜像自 rf/erf-thin-client：本核只构造 rf 调用、解析其输出、在导航前做
// 路径安全门禁、并以只读姿态渲染 receipt；绝不复制 Runtime/Factory/Server/Gate 权威。

const FORBIDDEN_FIELDS = [
  'source', 'sourceCode', 'sourceContent', 'sourceSnippet', 'fullFinding', 'findings',
];

const CLIENT_API_VERSION = 'rulefactory.io/v1';
const FINDING_VIEW_KIND = 'FindingView';

// 镜像 erf-thin-client 的 validate_relative_path：导航目标必须是工作区内相对路径。
function validateRelativePath(path) {
  if (typeof path !== 'string' || path.length === 0) {
    return { ok: false, reason: 'EMPTY_PATH' };
  }
  if (path.startsWith('/') || /^[A-Za-z]:/.test(path)) {
    return { ok: false, reason: 'ABSOLUTE_PATH' };
  }
  if (path.includes('\\')) {
    return { ok: false, reason: 'BACKSLASH' };
  }
  if (path.includes(':')) {
    return { ok: false, reason: 'COLON' };
  }
  // 控制字符（含 NUL、制表、换行等）一律拒绝。
  for (let i = 0; i < path.length; i += 1) {
    if (path.charCodeAt(i) < 0x20) {
      return { ok: false, reason: 'CONTROL_CHAR' };
    }
  }
  const segments = path.split('/');
  for (const seg of segments) {
    if (seg === '.' || seg === '..') {
      return { ok: false, reason: 'DOT_SEGMENT' };
    }
    if (seg.length === 0) {
      return { ok: false, reason: 'EMPTY_SEGMENT' };
    }
  }
  return { ok: true };
}

// 隐私门禁：拒绝任何携带源码/完整 Finding 的对象（递归）。
function findForbiddenField(value) {
  if (Array.isArray(value)) {
    for (const item of value) {
      const hit = findForbiddenField(item);
      if (hit) return hit;
    }
    return null;
  }
  if (value && typeof value === 'object') {
    for (const key of Object.keys(value)) {
      if (FORBIDDEN_FIELDS.includes(key)) return key;
      const hit = findForbiddenField(value[key]);
      if (hit) return hit;
    }
  }
  return null;
}

// 校验 FindingView 形状（闭世界：只允许既定字段），并做隐私与路径门禁。
function validateFindingView(view) {
  if (!view || typeof view !== 'object') {
    return { ok: false, reason: 'NOT_AN_OBJECT' };
  }
  const forbidden = findForbiddenField(view);
  if (forbidden) {
    return { ok: false, reason: 'FORBIDDEN_FIELD:' + forbidden };
  }
  if (view.apiVersion !== CLIENT_API_VERSION) {
    return { ok: false, reason: 'API_VERSION_MISMATCH' };
  }
  if (view.kind !== FINDING_VIEW_KIND) {
    return { ok: false, reason: 'KIND_MISMATCH' };
  }
  const allowed = new Set(['apiVersion', 'kind', 'findingId', 'ruleId', 'fingerprint', 'message', 'location']);
  for (const key of Object.keys(view)) {
    if (!allowed.has(key)) {
      return { ok: false, reason: 'UNKNOWN_FIELD:' + key };
    }
  }
  const loc = view.location;
  if (!loc || typeof loc !== 'object') {
    return { ok: false, reason: 'LOCATION_MISSING' };
  }
  const pathCheck = validateRelativePath(loc.path);
  if (!pathCheck.ok) {
    return { ok: false, reason: 'LOCATION_PATH:' + pathCheck.reason };
  }
  if (!Number.isInteger(loc.line) || loc.line < 1) {
    return { ok: false, reason: 'LOCATION_LINE' };
  }
  if (!Number.isInteger(loc.column) || loc.column < 1) {
    return { ok: false, reason: 'LOCATION_COLUMN' };
  }
  return { ok: true };
}

// ---- rf 调用参数构造（子命令均为空格分隔，见 rf 启动器） ----
function buildExplainArgs(findingPath, opts) {
  const o = opts || {};
  const args = ['explain', findingPath, '--format', o.format === 'text' ? 'text' : 'json'];
  if (o.ruleMeta) args.push('--rule-meta', o.ruleMeta);
  if (o.result) args.push('--result', o.result);
  if (o.assessment) args.push('--assessment', o.assessment);
  if (o.output) args.push('--output', o.output);
  return args;
}

function requireNonEmpty(name, value) {
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error('MISSING_ARG:' + name);
  }
}

function buildFeedbackDraftArgs(findingPath, opts) {
  const o = opts || {};
  requireNonEmpty('classification', o.classification);
  if (!['true-fp', 'not-fp', 'scope-issue', 'documentation'].includes(o.classification)) {
    throw new Error('BAD_CLASSIFICATION:' + o.classification);
  }
  requireNonEmpty('reason', o.reason);
  requireNonEmpty('actor', o.actor);
  const args = ['feedback', 'draft', findingPath,
    '--classification', o.classification, '--reason', o.reason, '--actor', o.actor];
  if (o.output) args.push('--output', o.output);
  return args;
}

function buildExceptionRequestArgs(findingPath, opts) {
  const o = opts || {};
  requireNonEmpty('scope', o.scope);
  requireNonEmpty('reason', o.reason);
  requireNonEmpty('actor', o.actor);
  const args = ['exception', 'request', findingPath,
    '--scope', o.scope, '--reason', o.reason, '--actor', o.actor];
  if (o.output) args.push('--output', o.output);
  return args;
}

// 提交动作要求显式 endpoint；为空则不构造（禁用自动发现）。
function buildSubmitArgs(kind, requestPath, opts) {
  const o = opts || {};
  requireNonEmpty('serverEndpoint', o.server);
  if (kind !== 'feedback' && kind !== 'exception') {
    throw new Error('BAD_SUBMIT_KIND:' + kind);
  }
  const args = [kind, 'submit', requestPath, '--server', o.server];
  if (Number.isInteger(o.timeoutMs) && o.timeoutMs > 0) {
    args.push('--timeout-ms', String(o.timeoutMs));
  }
  if (o.output) args.push('--output', o.output);
  return args;
}

function buildSubmissionStatusArgs(opts) {
  const o = opts || {};
  requireNonEmpty('serverEndpoint', o.server);
  const args = ['submission', 'status', '--server', o.server];
  if (o.requestId) args.push('--request-id', o.requestId);
  else if (o.artifactDigest) args.push('--artifact-digest', o.artifactDigest);
  else throw new Error('MISSING_ARG:request-id|artifact-digest');
  if (Number.isInteger(o.timeoutMs) && o.timeoutMs > 0) {
    args.push('--timeout-ms', String(o.timeoutMs));
  }
  return args;
}

// 解析 `rf explain --format text` 的 key=value 输出。
function parseExplainText(stdout) {
  const out = {};
  const lines = String(stdout || '').split('\n');
  for (const raw of lines) {
    const line = raw.trim();
    if (line.length === 0) continue;
    const eq = line.indexOf('=');
    if (eq <= 0) continue;
    out[line.slice(0, eq)] = line.slice(eq + 1);
  }
  return out;
}

// rf/erf-thin-client 退出码语义：0 可解释；2 输入非法；3 拒绝/PENDING/UNKNOWN。
function classifyExitCode(code) {
  if (code === 0) return 'OK';
  if (code === 2) return 'INVALID_INPUT';
  if (code === 3) return 'REFUSAL_OR_PENDING';
  return 'ERROR';
}

// 已知申请/提交状态到可读标签的映射；未知状态 fail closed（不渲染为成功）。
const STATUS_LABELS = {
  PROPOSED: '已起草（PROPOSED，尚未提交）',
  REQUESTED: '已生成申请（REQUESTED，尚未提交）',
  RECEIVED_ONLY: '仅已接收（RECEIVED_ONLY，非批准/非 Gate PASS/非 SCM 写入）',
  REPLAYED: '重复请求返回既有资源（REPLAYED）',
  PENDING: '待定（PENDING）',
  UNKNOWN: '结果未知（UNKNOWN，不作成功处理）',
};

function statusLabel(status) {
  return STATUS_LABELS[status] || ('未识别状态（' + String(status) + '，fail closed）');
}

// 只读渲染 SubmissionReceipt：三条虚假权威守卫必须恒为 false，否则视为契约违背。
function renderReceiptGuards(receipt) {
  const r = receipt || {};
  const violations = [];
  if (r.exceptionApproved === true) violations.push('exceptionApproved');
  if (r.gateMutated === true) violations.push('gateMutated');
  if (r.scmPublished === true) violations.push('scmPublished');
  return {
    status: statusLabel(r.status),
    guardsHold: violations.length === 0,
    violations,
    note: '提交回执仅代表“已接收/已记录”，绝不表示 Feedback 已采纳、Exception 已批准或 Gate PASS。',
  };
}

module.exports = {
  FORBIDDEN_FIELDS,
  CLIENT_API_VERSION,
  FINDING_VIEW_KIND,
  validateRelativePath,
  findForbiddenField,
  validateFindingView,
  buildExplainArgs,
  buildFeedbackDraftArgs,
  buildExceptionRequestArgs,
  buildSubmitArgs,
  buildSubmissionStatusArgs,
  parseExplainText,
  classifyExitCode,
  statusLabel,
  renderReceiptGuards,
  STATUS_LABELS,
};
