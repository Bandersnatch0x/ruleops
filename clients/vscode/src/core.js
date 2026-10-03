'use strict';
// ERF Thin Client 纯逻辑核（零依赖，与 VS Code API 解耦，可离线单测）。
// 权威边界镜像自 rf/erf-thin-client：本核只构造 rf 调用、解析其输出、在导航前做
// 路径安全门禁、并以只读姿态渲染 receipt；绝不复制 Runtime/Factory/Server/Gate 权威。

const FORBIDDEN_FIELDS = [
  'source', 'sourceCode', 'sourceContent', 'sourceSnippet', 'fullFinding', 'findings',
];

const CLIENT_API_VERSION = 'rulefactory.io/v1';
const FINDING_VIEW_KIND = 'FindingView';
// Local CLI contract, not a remotely invented classification API.
const CLASSIFICATIONS = Object.freeze(['true-fp', 'not-fp', 'scope-issue', 'documentation']);

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
    if (path.charCodeAt(i) < 0x20 || (path.charCodeAt(i) >= 0x7f && path.charCodeAt(i) <= 0x9f)) {
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
  for (const name of ['findingId', 'ruleId', 'fingerprint', 'message']) {
    if (typeof view[name] !== 'string' || !view[name].trim() || /[\u0000-\u001f\u007f-\u009f]/.test(view[name])) return { ok: false, reason: 'INVALID_FIELD:' + name };
  }
  const loc = view.location;
  if (!loc || typeof loc !== 'object') {
    return { ok: false, reason: 'LOCATION_MISSING' };
  }
  if (Object.keys(loc).length !== 3 || !['path', 'line', 'column'].every(key => Object.hasOwn(loc, key))) return { ok: false, reason: 'LOCATION_FIELDS' };
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
  if (o.authorityTrust && !o.result) throw new Error('AUTHORITY_TRUST_REQUIRES_RESULT');
  if (o.result) args.push('--result', o.result);
  if (o.authorityTrust) args.push('--authority-trust', o.authorityTrust);
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
  if (!CLASSIFICATIONS.includes(o.classification)) {
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
  if (o.requestId && o.artifactDigest) throw new Error('AMBIGUOUS_STATUS_SELECTOR');
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
  SUBMITTED: '已提交（仅已接收，非批准）',
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


// Facts are supplied by the filesystem adapter; this module never resolves files,
// reads config or grants scope authority. CLI remains the request validator.
function scopeCandidates(finding, config = {}) {
  if (!validateFindingView(finding).ok) return [];
  const file = finding.location.path;
  const concrete = value => validateRelativePath(value).ok && value === value.trim() && !/[*?\[\]{}!]/.test(value);
  if (!concrete(file)) return [];
  const options = [{ value: file, source: 'file', broader: false }];
  const add = (directory, source) => {
    if (!concrete(directory) || !file.startsWith(directory + '/')) return;
    const value = directory + '/**';
    if (!options.some(option => option.value === value)) options.push({ value, source, broader: true });
  };
  if (file.includes('/')) add(file.slice(0, file.lastIndexOf('/')), 'directory');
  for (const [key, source] of [['modules', 'module'], ['source_roots', 'source_root']]) {
    for (const directory of Array.isArray(config?.[key]) ? config[key] : []) add(directory, source);
  }
  return options;
}
function projectScopeOptions(finding, config, facts) {
  const options = scopeCandidates(finding, config);
  if (!options.length || !facts.files.includes(options[0].value)) return [];
  return options.filter(option => option.broader
    ? facts.directories.includes(option.value.slice(0, -3)) : true);
}
function selectScope(options, value, confirmed = false) {
  const selected = options.find(option => option.value === value);
  if (!selected) throw new Error('STALE_SCOPE_SELECTION');
  if (selected.broader && !confirmed) throw new Error('SCOPE_EXPANSION_REQUIRES_CONFIRMATION');
  return selected.value;
}
function projectSavedRequest(request) {
  const forbidden = findForbiddenField(request);
  if (forbidden) throw new Error('FORBIDDEN_FIELD:' + forbidden);
  const kind = { FeedbackDraft: 'feedback', ExceptionRequest: 'exception' }[request?.kind];
  if (!kind || request.apiVersion !== CLIENT_API_VERSION || request.schemaVersion !== (kind === 'feedback' ? 'feedback-draft-v1' : 'exception-request-v1')
    || request.status !== (kind === 'feedback' ? 'PROPOSED' : 'REQUESTED')
    || !/^sha256:[a-f0-9]{64}$/.test(request.requestId) || !/^sha256:[a-f0-9]{64}$/.test(request.digest)) {
    throw new Error('INVALID_SAVED_REQUEST');
  }
  return { kind, requestId: request.requestId, artifactDigest: request.digest, status: request.status };
}
function selectRequest(records, path, kind) {
  const selected = records.find(record => record.path === path && (!kind || record.kind === kind));
  if (!selected) throw new Error('STALE_REQUEST_SELECTION');
  return selected;
}
function statusSelection(request, selector) {
  if (!request || !['requestId', 'artifactDigest'].includes(selector)) throw new Error('STALE_STATUS_SELECTION');
  requireNonEmpty(selector, request[selector]);
  return { [selector]: request[selector] };
}
function validateSubmissionResponse(receipt, request, statusQuery = false) {
  if (!receipt || typeof receipt !== 'object' || Array.isArray(receipt) || findForbiddenField(receipt)
    || receipt.apiVersion !== CLIENT_API_VERSION || receipt.schemaVersion !== (statusQuery ? 'submission-status-v1' : 'submission-receipt-v1')) throw new Error('INVALID_RECEIPT');
  if (statusQuery) {
    if (receipt.kind !== 'SubmissionStatus' || !['UNKNOWN', 'RECEIVED_ONLY'].includes(receipt.authority) || receipt.automaticRetry !== false
      || !['RECEIVED_ONLY', 'REPLAYED', 'PENDING', 'UNKNOWN'].includes(receipt.status)) throw new Error('INVALID_STATUS_RECEIPT');
  } else if (receipt.kind !== 'SubmissionReceipt' || !['SUBMITTED', 'REPLAYED'].includes(receipt.status)
    || ['exceptionApproved', 'gateMutated', 'scmPublished'].some(key => receipt[key] !== false)
    || receipt.governanceOutcome !== 'RECEIVED_ONLY') throw new Error('INVALID_RECEIPT_GUARDS');
  if (statusQuery && !receipt.requestId && !receipt.artifactDigest) throw new Error('RECEIPT_IDENTITY_MISSING');
  for (const key of ['requestId', 'artifactDigest']) {
    if ((!statusQuery || receipt[key] !== undefined) && receipt[key] !== request[key]) throw new Error('RECEIPT_IDENTITY_MISMATCH:' + key);
  }
  return receipt;
}


// Registry metadata and release pins are deliberately separate inputs. No network lives here.
function registryText(value, name) {
  requireNonEmpty(name, value);
  if (/[\u0000-\u001f\u007f]/.test(value)) throw new Error('INVALID_REGISTRY_FIELD:' + name);
  return value;
}
function registryResource(resource) {
  if (!['rules', 'standards'].includes(resource)) throw new Error('REGISTRY_TYPE_INVALID');
  return resource;
}
function registryConnectionArgs(opts) {
  const profile = opts.connectionProfile || '';
  const server = opts.server || '';
  if (!!profile === !!server) throw new Error('REGISTRY_CONNECTION_REQUIRED_EXACTLY_ONE');
  if (server) {
    const uri = new URL(server);
    if (!['http:', 'https:'].includes(uri.protocol) || uri.username || uri.password || uri.search || uri.hash) throw new Error('INVALID_REGISTRY_ENDPOINT');
  }
  const args = profile ? ['--connection-profile', registryText(profile, 'connectionProfile')]
    : ['--server', registryText(server, 'server')];
  if (!Number.isInteger(opts.timeoutMs) || opts.timeoutMs <= 0) throw new Error('INVALID_TIMEOUT');
  return [...args, '--timeout-ms', String(opts.timeoutMs)];
}
function projectRegistryPins(document) {
  if (!document || !Array.isArray(document.pins)) throw new Error('INVALID_RELEASE_PINS');
  return document.pins.map(pin => {
    if (!pin || typeof pin !== 'object') throw new Error('INVALID_RELEASE_PIN');
    const result = { resource: registryResource(pin.resource) };
    for (const key of ['id', 'version', 'expectedDigest', 'provenance']) result[key] = registryText(pin[key], key);
    if (!/^sha256:[a-f0-9]{64}$/.test(result.expectedDigest)) throw new Error('INVALID_EXPECTED_DIGEST');
    if (pin.deliveryDescriptor !== undefined) result.deliveryDescriptor = registryText(pin.deliveryDescriptor, 'deliveryDescriptor');
    return Object.freeze(result);
  });
}
function selectRegistryPin(pins, selection, resource) {
  const pin = pins.find((_, index) => String(index) === selection);
  if (!pin || pin.resource !== resource) throw new Error('INDEPENDENT_PIN_REQUIRED');
  return pin;
}
function buildRegistryListArgs(resource, opts) {
  registryResource(resource);
  if (!Number.isInteger(opts.limit) || opts.limit < 1 || opts.limit > 256) throw new Error('REGISTRY_LIMIT_INVALID');
  const args = ['registry', 'list', resource, ...registryConnectionArgs(opts), '--limit', String(opts.limit)];
  if (opts.cursor !== undefined) {
    if (typeof opts.cursor !== 'string') throw new Error('REGISTRY_CURSOR_INVALID');
    args.push('--cursor-json', JSON.stringify(opts.cursor)); // NUL stays escaped in one argv.
  }
  return args;
}
function buildRegistryShowArgs(pin, opts) {
  const checked = projectRegistryPins({ pins: [pin] })[0];
  return ['registry', 'show', checked.resource, checked.id, '--version', checked.version,
    '--expected-digest', checked.expectedDigest, ...registryConnectionArgs(opts)];
}
function buildRegistrySyncArgs(pin, descriptor, repoRoot, opts) {
  if (pin?.resource !== 'rules') throw new Error('SYNC_RULES_ONLY');
  const args = buildRegistryShowArgs(pin, opts);
  args[1] = 'sync';
  return [...args, '--delivery-descriptor', registryText(descriptor, 'deliveryDescriptor'), '--repo-root', registryText(repoRoot, 'repoRoot')];
}
function registryEnvelope(receipt) {
  const registry = receipt?.registry;
  if (receipt?.status !== 'RECEIVED_ONLY' || receipt.authority !== 'UNKNOWN'
    || !['FORMAT_ONLY', 'PIN_MATCH', 'UNKNOWN'].includes(receipt.digestVerification)) throw new Error('REGISTRY_RESPONSE_INVALID');
  return registryData(registry);
}
function registryData(registry) {
  if (registry?.apiVersion !== CLIENT_API_VERSION || registry.schemaVersion !== 'asset-registry-v1'
    || registry.authority !== 'UNKNOWN' || registry.requestId !== 'registry-read') throw new Error('REGISTRY_RESPONSE_INVALID');
  return registry;
}
function registryItem(item, resource) {
  if (item?.type !== registryResource(resource) || !/^sha256:[a-f0-9]{64}$/.test(item.digest)) throw new Error('REGISTRY_RESPONSE_INVALID');
  for (const key of ['id', 'version', 'state']) registryText(item[key], key);
  return Object.freeze({ type: item.type, id: item.id, version: item.version, state: item.state, digest: item.digest });
}
function projectRegistryPage(receipt, resource, cursor = '') {
  const data = registryEnvelope(receipt);
  if (receipt.digestVerification === 'PIN_MATCH' || !Array.isArray(data.items) || typeof data.nextCursor !== 'string'
    || data.cursor !== cursor || typeof data.hasMore !== 'boolean'
    || (data.hasMore && (!data.items.length || data.nextCursor === cursor))) throw new Error('REGISTRY_PAGE_INVALID');
  return { items: data.items.map(item => registryItem(item, resource)), nextCursor: data.nextCursor, hasMore: data.hasMore };
}
function matchRegistryPin(data, pin) {
  const item = registryItem(data.resource, pin.resource);
  if (item.id !== pin.id || item.version !== pin.version) throw new Error('REGISTRY_IDENTITY_MISMATCH · UNKNOWN · 未安装');
  if (item.digest !== pin.expectedDigest) throw new Error('REGISTRY_DIGEST_MISMATCH · UNKNOWN · 未安装');
}
function renderRegistryShow(receipt, pin) {
  matchRegistryPin(registryEnvelope(receipt), pin);
  return `${receipt.digestVerification} · authority=UNKNOWN · 未安装 / 不代表可执行\n${pin.id} @ ${pin.version}\n${pin.expectedDigest}\n独立来源：${pin.provenance}`;
}
function renderRegistrySync(receipt, pin) {
  if (!receipt || receipt.authority !== 'UNKNOWN') throw new Error('REGISTRY_SYNC_RESPONSE_INVALID');
  if (receipt.status !== 'CACHED') return 'UNKNOWN · 未安装 / 离线可用性未知';
  if (receipt.digestVerification !== 'PIN_MATCH' || receipt.artifactKind !== 'SignedRuleBundle'
    || !/^sha256:[a-f0-9]{64}$/.test(receipt.bundleDigest) || typeof receipt.destination !== 'string'
    || !receipt.destination || receipt.overwritePolicy !== 'IMMUTABLE_DIGEST_ADDRESSED'
    || typeof receipt.offlineAvailable !== 'boolean' || !receipt.provenance
    || typeof receipt.provenance.descriptor !== 'string' || typeof receipt.provenance.trustRoot !== 'string') throw new Error('REGISTRY_SYNC_RESPONSE_INVALID');
  matchRegistryPin(registryData(receipt.registry), pin);
  return `CACHED · PIN_MATCH · authority=UNKNOWN · 缓存非安装 / 非 Gate PASS\nartifact：${receipt.artifactKind}\nbundleDigest：${receipt.bundleDigest}\ndestination：${receipt.destination}\noverwritePolicy：${receipt.overwritePolicy}\nofflineAvailable：${receipt.offlineAvailable}\nprovenance：${JSON.stringify(receipt.provenance)}\n后续扫描仍需 ExecutionLock、签名和撤销验证。`;
}

// ---- 统一命令面：机械地从同一份 map params 构造 rf argv ----
// 两端（VS Code core.js / IntelliJ ThinClientCore.java）消费的是同一份
// clients/ide-command-map.json#commands[].params；本层不新增网络或业务流程。

function requireControlFree(value, name) {
  if (/[\u0000-\u001f\u007f]/.test(value)) throw new Error('INVALID_CONTROL:' + name);
  return value;
}
function digestValue(value, name) {
  requireControlFree(value, name);
  if (!/^sha256:[a-f0-9]{64}$/.test(value)) throw new Error('INVALID_DIGEST:' + name);
  return value;
}
function hexDigestValue(value, name) {
  requireControlFree(value, name);
  if (!/^[a-f0-9]{64}$/.test(value)) throw new Error('INVALID_HEX_DIGEST:' + name);
  return value;
}
function numberValue(value, name) {
  requireControlFree(String(value), name);
  if (!Number.isInteger(value) && !/^\d+$/.test(String(value))) throw new Error('INVALID_NUMBER:' + name);
  return String(value);
}
function splitCsv(value, name) {
  const parts = Array.isArray(value) ? value : String(value).split(',');
  return parts.map(part => requireControlFree(String(part).trim(), name)).filter(part => part.length > 0);
}

// 参数值 → argv：按 map 声明的 params 顺序机械展开（位置参数 + flag）。
// 子命令前缀来自 commandTemplate 且与 rf 启动器的分派词一致（area [+ action]）。
// 已知分派词集合 = rf 启动器 case 分支；遇到其它 token（占位符/资源选择）即停，
// 避免把 rules|standards、PATH、FILE... 这类 params 占位误入前缀。
const RF_DISPATCH_WORDS = new Set([
  'init', 'doctor', 'config', 'registry', 'standard', 'rule', 'coverage', 'pilot', 'decision',
  'center', 'status', 'explain', 'feedback', 'exception', 'submission', 'scan',
  'validate', 'show', 'list', 'sync', 'generate', 'test', 'analyze', 'report', 'delivery-check',
  'baseline', 'observe', 'governance', 'window-report', 'package', 'adjudicate', 'evaluate',
  'check', 'contract-check', 'deployment-check', 'draft', 'request', 'submit',
]);
function commandPrefix(cmd) {
  const template = String(cmd.commandTemplate || '');
  const body = template.startsWith('rf ') ? template.slice(3) : template;
  const words = [];
  for (const token of body.split(/\s+/)) {
    if (!RF_DISPATCH_WORDS.has(token)) break;
    words.push(token);
    // evidence 是名词组前缀（rf pilot evidence package），不属于分派词本身。
    if (words[words.length - 1] === 'evidence') continue;
  }
  // pilot evidence package/adjudicate：'evidence' 不在集合，循环会停在 pilot 后；
  // 这里补上 "pilot evidence X" 的名词组。
  if (words.length === 1 && words[0] === 'pilot' && body.startsWith('pilot evidence ')) {
    const third = body.split(/\s+/)[2];
    if (third === 'package' || third === 'adjudicate') words.push('evidence', third);
  }
  return words;
}

function buildMappedArgs(cmd, values) {
  if (!cmd || !Array.isArray(cmd.params)) throw new Error('MISSING_COMMAND_PARAMS');
  if (cmd.commandId === 'rf.scan') return buildScanArgs(cmd, values);
  const input = values || {};
  const args = commandPrefix(cmd);
  for (const p of cmd.params) {
    const raw = input[p.name];
    if (p.kind === 'bool') {
      const on = raw === true || raw === 'true' || raw === '1';
      if (on) args.push(p.flag);
      continue;
    }
    if (raw === undefined || raw === null || raw === '') {
      if (p.required) throw new Error('MISSING_ARG:' + p.name);
      continue;
    }
    const items = Array.isArray(raw) ? raw : (p.kind === 'csv' ? splitCsv(raw, p.name) : [raw]);
    for (const item of items) {
      let sv;
      switch (p.kind) {
        case 'enum': {
          sv = requireControlFree(String(item), p.name);
          if (!p.values.includes(sv)) throw new Error('BAD_ENUM:' + p.name + '=' + sv);
          break;
        }
        case 'digest': sv = (p.hexDigest ? hexDigestValue : digestValue)(String(item), p.name); break;
        case 'number': sv = numberValue(item, p.name); break;
        default: {
          sv = requireControlFree(String(item), p.name);
          // CLI 路径参数由宿主提供，允许绝对路径（如 --repo-root）；相对路径门禁属工作区导航层。
        }
      }
      if (p.flag) args.push(p.flag, sv);
      else args.push(sv);
    }
  }
  return args;
}

// scan 路线互斥、base/head 成对、JSON 结构化输出；其余 flag 由同一 params 展开。
function buildScanArgs(cmd, values) {
  const input = values || {};
  const members = { 'rule-id': 'ruleId', candidate: 'candidate', 'delivery-descriptor': 'deliveryDescriptor' };
  const route = input.route || (input.ruleId ? 'rule-id' : input.candidate ? 'candidate' : input.deliveryDescriptor ? 'delivery-descriptor' : null);
  if (!route || !members[route]) throw new Error('SCAN_ROUTE_REQUIRED');
  const memberName = members[route];
  if (input[memberName] === undefined || input[memberName] === null || input[memberName] === '') throw new Error('MISSING_ARG:' + memberName);
  const extraMembers = Object.keys(members).filter(r => r !== route).map(r => members[r])
    .filter(name => input[name] !== undefined && input[name] !== null && input[name] !== '');
  if (extraMembers.length) throw new Error('SCAN_ROUTE_CONFLICT');
  if (Boolean(input.base) !== Boolean(input.head)) throw new Error('BASE_HEAD_MUST_BE_PAIRED');
  const rest = cmd.params.filter(p =>
    p.name === 'route' || p.name === memberName || p.name === 'base' || p.name === 'head');
  const filtered = { ...input };
  for (const name of ['ruleId', 'candidate', 'deliveryDescriptor', 'base', 'head']) if (name !== memberName && !['base', 'head'].includes(name)) filtered[name] = undefined;
  const args = commandPrefix(cmd);
  for (const p of cmd.params) {
    if (rest.includes(p)) continue;
    const raw = p.name === 'base' || p.name === 'head' ? input[p.name] : filtered[p.name];
    if (p.kind === 'bool') {
      const on = raw === true || raw === 'true' || raw === '1';
      if (on) args.push(p.flag);
      continue;
    }
    if (raw === undefined || raw === null || raw === '') {
      if (p.required) throw new Error('MISSING_ARG:' + p.name);
      continue;
    }
    const items = Array.isArray(raw) ? raw : (p.kind === 'csv' ? splitCsv(raw, p.name) : [raw]);
    for (const item of items) {
      let sv;
      switch (p.kind) {
        case 'enum': sv = requireControlFree(String(item), p.name);
          if (!p.values.includes(sv)) throw new Error('BAD_ENUM:' + p.name + '=' + sv); break;
        case 'digest': sv = (p.hexDigest ? hexDigestValue : digestValue)(String(item), p.name); break;
        case 'number': sv = numberValue(item, p.name); break;
        default: sv = requireControlFree(String(item), p.name);
          // CLI 路径参数由宿主提供，允许绝对路径（如 --repo-root）；相对路径门禁属工作区导航层。
      }
      if (p.flag) args.push(p.flag, sv);
      else args.push(sv);
    }
  }
  const routeFlag = { 'rule-id': '--rule-id', candidate: '--candidate', 'delivery-descriptor': '--delivery-descriptor' }[route];
  args.push(routeFlag, input[memberName]);
  if (input.base) args.push('--base', input.base, '--head', input.head);
  return args;
}

// ---- 扫描结果工作台：解析 rf scan 结构化输出，权威来自 rf-runner ----
// 关键：绝不把 scan exit=0 当作 PASS；展示以 ScanResult.status/gateDecision/
// authority 为准，ScanFailure 显示的 exitCode 就是 CLI 契约等级。

function classifyScanExit(code) {
  if (code === 0) return 'OK';
  if (code === 2) return 'INVALID_INPUT';
  if (code === 3) return 'INCONCLUSIVE_OR_REFUSED';
  if (code === 4) return 'TRUST_FAILED';
  return 'ERROR';
}

// 扫描不是"命令退出成功=通过"的简单开关：status 取 scan-output 的 DisplayStatus。
const SCAN_STATUS_LABELS = {
  PASS: 'PASS（Gate 通过）',
  FAIL: 'FAIL（存在未豁免 finding，Gate 阻止）',
  INCONCLUSIVE: 'INCONCLUSIVE（证据不足，Gate 未决）',
  LOCAL_UNATTESTED: 'LOCAL_UNATTESTED（本地未证明，非生产 Gate 证据）',
};

// 把既有 ScanResult/ScanFailure 结构化对象收敛为归一化视图。
// 与 Java 端 ThinClientCore 消费同一契约字段，供跨端 parity 使用。
function normalizeScanReport(raw) {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return { ok: false, reason: 'NOT_OBJECT' };
  if (findForbiddenField(raw)) return { ok: false, reason: 'FORBIDDEN_FIELD' };
  const kind = raw.kind;
  if (kind === 'ScanResult') {
    const fields = ['apiVersion', 'kind', 'status', 'gateDecision', 'authority', 'trust', 'ruleId', 'findingRecords', 'diagnostics', 'progress', 'resultDigest'];
    if (!fields.every(f => Object.hasOwn(raw, f))) return { ok: false, reason: 'SCAN_RESULT_INCOMPLETE' };
    if (!Object.hasOwn(SCAN_STATUS_LABELS, raw.status)) return { ok: false, reason: 'SCAN_STATUS_UNKNOWN:' + raw.status };
    return { ok: true, kind: 'ScanResult', status: raw.status, gateDecision: raw.gateDecision,
      authority: raw.authority, trust: raw.trust, ruleId: raw.ruleId,
      planned: raw.planned, executed: raw.executed, failed: raw.failed,
      findings: Array.isArray(raw.findingRecords) ? raw.findingRecords : [],
      diagnostics: Array.isArray(raw.diagnostics) ? raw.diagnostics : [],
      progress: Array.isArray(raw.progress) ? raw.progress : [],
      resultDigest: raw.resultDigest };
  }
  if (kind === 'ScanFailure') {
    const fields = ['apiVersion', 'kind', 'exitCode', 'diagnostics', 'progress', 'diagnosticDigest'];
    if (!fields.every(f => Object.hasOwn(raw, f))) return { ok: false, reason: 'SCAN_FAILURE_INCOMPLETE' };
    return { ok: true, kind: 'ScanFailure', exitCode: Number(raw.exitCode),
      diagnostics: Array.isArray(raw.diagnostics) ? raw.diagnostics : [],
      progress: Array.isArray(raw.progress) ? raw.progress : [] };
  }
  return { ok: false, reason: 'SCAN_KIND_UNKNOWN:' + String(kind) };
}

function parseScanStdout(stdout) {
  const text = String(stdout || '').trim();
  if (!text) return { ok: false, reason: 'EMPTY_OUTPUT' };
  let raw;
  try { raw = JSON.parse(text); }
  catch (error) { return { ok: false, reason: 'INVALID_JSON' }; }
  return normalizeScanReport(raw);
}

function renderScanReport(parsed, exitCode, stderr) {
  if (parsed && !parsed.ok) parsed = normalizeScanReport(parsed);
  if (!parsed || !parsed.ok) return 'scan 输出无法按既有 ScanResult/ScanFailure 契约解析：' + (parsed && parsed.reason) + '\nexit=' + exitCode;
  const progressLines = parseProgressLines(stderr);
  const progressPart = progressLines.length ? '\n' + progressLines.join('\n') : '';
  if (parsed.kind === 'ScanFailure') {
    const diags = parsed.diagnostics.map(d =>
      `diagnostic code=${d.code} stage=${d.stage} retry=${d.retry || ''} message=${d.message} next_step=${d.nextStep || ''}`).join('\n');
    return `扫描未完成（ScanFailure）出口码=${parsed.exitCode}（${classifyScanExit(parsed.exitCode)}）\n${diags}${progressPart}`;
  }
  const statusLine = SCAN_STATUS_LABELS[parsed.status] + (parsed.gateDecision ? ' · gate=' + parsed.gateDecision : '')
    + ' · authority=' + parsed.authority + ' · trust=' + parsed.trust;
  const header = `scan ${parsed.status} — 不做成 PASS：请以 status/gate/authority 为准\n${statusLine}\nrule_id=${parsed.ruleId} planned=${parsed.planned} executed=${parsed.executed} failed=${parsed.failed}\nresult_digest=${parsed.resultDigest}`;
  const findings = parsed.findings.map(f => {
    const loc = f.location || {};
    const where = loc.path ? (loc.line ? `${loc.path}:${loc.line}:${loc.column || 1}` : loc.path) : ('identity ' + (loc.identityDigest || ''));
    return `finding id=${f.findingId} rule=${f.ruleId} location=${where} digest=${f.findingDigest} message=${f.message}`;
  }).join('\n');
  const diags = parsed.diagnostics.map(d => `diagnostic code=${d.code} stage=${d.stage} retry=${d.retry || ''} message=${d.message} next_step=${d.nextStep || ''}`).join('\n');
  return [header, findings ? 'findings：\n' + findings : 'findings：无', diags ? 'diagnostics：\n' + diags : '', 'exit=' + exitCode].filter(Boolean).join('\n') + progressPart;
}

// stderr progress 行：`progress stage=... outcome=... elapsed_nanos=...`（长任务不 5 秒截断）。
function parseProgressLines(stderr) {
  return String(stderr || '').split('\n').map(line => line.trim()).filter(line => line.startsWith('progress '));
}

function mapCommand(commandMap, commandId) {
  if (!commandMap || !Array.isArray(commandMap.commands)) return null;
  return commandMap.commands.find(c => c.commandId === commandId) || null;
}

module.exports = {
  projectRegistryPins, selectRegistryPin, buildRegistryListArgs, buildRegistryShowArgs, buildRegistrySyncArgs, projectRegistryPage, renderRegistryShow, renderRegistrySync,
  CLASSIFICATIONS, scopeCandidates, projectScopeOptions, selectScope, projectSavedRequest, selectRequest, statusSelection, validateSubmissionResponse,
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
  classifyScanExit,
  buildMappedArgs,
  buildScanArgs,
  normalizeScanReport,
  parseScanStdout,
  renderScanReport,
  parseProgressLines,
  SCAN_STATUS_LABELS,
  mapCommand,
  statusLabel,
  renderReceiptGuards,
  STATUS_LABELS,
};
