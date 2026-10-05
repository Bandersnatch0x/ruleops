'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const { randomBytes, createHash } = require('node:crypto');
const core = require('./core');
const { resolveSafeWorkspaceFile, resolveSafeWorkspaceDirectory, resolveSafeSelectedPath } = require('./path-safety');
const { runRf } = require('./rf-invoker');
const { html } = require('./sidebar-webview');

const SCENARIOS = [
  { id: 'erf.showFinding', label: '展示 Finding', section: 'scan_findings', fields: [] },
  { id: 'erf.navigateToLocation', label: '导航到位置', section: 'scan_findings', fields: [] },
  { id: 'erf.explainFinding', cli: 'rf.explain', label: '解释 Finding', section: 'scan_findings', fields: [] },
  { id: 'erf.scan', cli: 'rf.scan', label: '扫描结果工作台', section: 'scan_findings',
    fields: ['route', 'ruleId', 'candidate', 'deliveryDescriptor', 'format', 'progress', 'output', 'deadlineMs', 'base', 'head', 'host', 'resolver', 'runtimeHome'],
    scan: true, preview: true, autoRepoRoot: true,
    previewNote: '将委托 rf-runner 执行扫描；可能运行 Backend/Resolver 并写缓存/输出文件。exit=0 不代表 Gate PASS，以结果 status/gate/authority 为准。' },
  { id: 'erf.draftFeedback', cli: 'rf.feedback.draft', label: '起草反馈', section: 'feedback_exceptions', fields: ['classification', 'reason', 'actor'] },
  { id: 'erf.requestException', cli: 'rf.exception.request', label: '申请例外', section: 'feedback_exceptions', fields: ['scope', 'reason', 'actor'] },
  { id: 'erf.submitFeedback', cli: 'rf.feedback.submit', label: '提交反馈', section: 'feedback_exceptions', fields: ['request'], server: true },
  { id: 'erf.submitException', cli: 'rf.exception.submit', label: '提交例外', section: 'feedback_exceptions', fields: ['request'], server: true },
  { id: 'erf.submissionStatus', cli: 'rf.submission.status', label: '查询提交状态', section: 'feedback_exceptions', fields: ['request', 'selector'], server: true },
  { id: 'erf.registryList', cli: 'rf.registry.list', label: '浏览规则库（元数据）', section: 'registry', fields: ['resource', 'limit'], registry: true },
  { id: 'erf.registryShow', cli: 'rf.registry.show', label: '核对独立发布 pin', section: 'registry', fields: ['resource', 'pin'], registry: true },
  { id: 'erf.registrySync', cli: 'rf.registry.sync', label: '缓存签名规则包', section: 'registry', fields: ['resource', 'pin'], registry: true },
  { id: 'erf.doctor', cli: 'rf.doctor', label: '环境诊断', section: 'workspace', fields: ['format', 'scanProbe'], preview: true, autoRepoRoot: true,
    previewNote: '只读诊断；仅在显式填写 scan-probe 时执行该外部命令。' },
  { id: 'erf.configValidate', cli: 'rf.config.validate', label: '校验工作区配置', section: 'workspace', fields: ['config'], autoRepoRoot: true },
  { id: 'erf.configShow', cli: 'rf.config.show', label: '查看有效配置', section: 'workspace', fields: ['config', 'format'], autoRepoRoot: true },
  { id: 'erf.standardValidate', cli: 'rf.standard.validate', label: '校验标准文件', section: 'authoring_coverage', fields: ['files'], preview: true,
    previewNote: '运行创作校验器（只读输入）；仓库脚本可能准备 Maven classpath/临时文件。' },
  { id: 'erf.ruleValidate', cli: 'rf.rule.validate', label: '校验规则文件', section: 'authoring_coverage', fields: ['files'], preview: true,
    previewNote: '运行创作校验器（只读输入）；仓库脚本可能准备 Maven classpath/临时文件。' },
];
const LABELS = { ready: '就绪', unconfigured: '未配置', unavailable: '不可用', running: '运行中', failed: '失败', cancelled: '已取消', completed: '已完成（非批准）' };
const sceneIcon = scene => scene.scan ? 'search' : scene.registry ? 'package' : scene.server ? 'cloud-upload' : scene.section === 'feedback_exceptions' ? 'edit' : scene.section === 'workspace' ? 'tools' : scene.section === 'authoring_coverage' ? 'checklist' : scene.id === 'erf.navigateToLocation' ? 'go-to-file' : 'inspect';

// 需要 digest-only FindingView 输入的场景；其余场景（scan/config/doctor/校验）不挑 Finding。
const FINDING_SCENES = new Set(['erf.showFinding', 'erf.navigateToLocation', 'erf.explainFinding', 'erf.draftFeedback', 'erf.requestException']);
const needsFindingScene = id => FINDING_SCENES.has(id);

function validateMessage(message, scene) {
  if (!message || typeof message !== 'object' || Array.isArray(message)) throw new Error('INVALID_MESSAGE');
  const keys = Object.keys(message);
  if (['ready', 'pickFile', 'workspace', 'cancel', 'refresh', 'settings', 'pickAuthority', 'clearAuthority', 'pickReleasePins', 'pickDescriptor', 'nextPage'].includes(message.type) && keys.length === 1) return message;
  if (!['run', 'edit'].includes(message.type) || keys.length !== 2 || !keys.includes('fields') || !scene) throw new Error('INVALID_MESSAGE');
  const fields = message.fields;
  if (!fields || typeof fields !== 'object' || Array.isArray(fields)) throw new Error('INVALID_FIELDS');
  for (const [key, value] of Object.entries(fields)) {
    if (!scene.fields.includes(key) || typeof value !== 'string' || value.length > 16384 || value.includes('\0')) throw new Error('INVALID_FIELD:' + key);
  }
  if (message.type === 'run') {
    for (const name of ['actor', 'reason']) {
      if (scene.fields.includes(name) && !fields[name]?.trim()) throw new Error('MISSING_ARG:' + name);
    }
  }
  return message;
}

function createSidebar(vscode, context, commandMap, invoke = runRf) {
  const events = new vscode.EventEmitter();
  let scene = SCENARIOS[0];
  let root = vscode.workspace.workspaceFolders?.length === 1 ? vscode.workspace.workspaceFolders[0].uri.fsPath : null;
  let file = null;
  let fields = {};
  let scopes = [];
  let scopeNote = '';
  let selectedRequestPath = null;
  let authority = {};
  let registryPage = null;
  let registryType = 'rules';
  let releasePins = [];
  let releaseSource = '';
  let descriptor = '';
  function clearRegistry() { registryPage = null; releasePins = []; releaseSource = ''; descriptor = ''; }
  const registryConnection = c => c.connectionProfile ? { connectionProfile: c.connectionProfile, timeoutMs: c.requestTimeoutMs }
    : { server: c.serverEndpoint, timeoutMs: c.requestTimeoutMs };

  let records = context.workspaceState.get('erf.requests', []);
  const checksum = text => createHash('sha256').update(text).digest('hex');
  const endpointKey = () => checksum(config().serverEndpoint);
  const availableRequests = () => records.filter(record => record.root === root
    && (!record.endpointKey || record.endpointKey === endpointKey())
    && (!scene.id.includes('submit') || record.kind === (scene.id === 'erf.submitFeedback' ? 'feedback' : 'exception')));
  async function remember(record) {
    records = [record, ...records.filter(r => !(r.root === record.root && r.path === record.path))].slice(0, 100);
    await context.workspaceState.update('erf.requests', records);
  }
  function clearSelection() { clearRegistry(); file = null; scopes = []; authority = {}; selectedRequestPath = null; fields = {}; }
  async function readFinding() {
    if (!file) throw new Error('请先选择 FindingView JSON');
    const finding = JSON.parse(await fs.readFile(file, 'utf8'));
    const check = core.validateFindingView(finding);
    if (!check.ok) throw new Error('FindingView 被拒绝：' + check.reason);
    return finding;
  }
  async function readRequest(record) {
    const savedPath = resolveSafeSelectedPath(record.path);
    const content = await fs.readFile(savedPath, 'utf8');
    const identity = core.projectSavedRequest(JSON.parse(content));
    if (checksum(content) !== record.contentDigest || identity.requestId !== record.requestId
      || identity.artifactDigest !== record.artifactDigest) throw new Error('SAVED_REQUEST_CHANGED: 请重新保存请求');
    return record;
  }
  async function loadScopes(finding, signal) {
    // A config failure is visible; it cannot invent module roots or a whole-repo fallback.
    resolveSafeWorkspaceFile(root, finding.location.path);
    const result = await invoke(['config', 'show', '--format', 'json'], config(), root, signal);
    if (signal.aborted) throw new Error('CANCELLED');
    let effective = {};
    let note = '';
    if (result.reason || result.code !== 0) note = '有效配置不可用，未提供 module/source_root：' + (result.reason || result.stderr);
    else {
      const parsed = JSON.parse(result.stdout);
      if (!parsed.config || !Array.isArray(parsed.config.modules) || !Array.isArray(parsed.config.source_roots)) throw new Error('INVALID_CONFIG_RESPONSE');
      effective = parsed.config;
    }
    const facts = { files: [finding.location.path], directories: [] };
    const rejected = [];
    for (const option of core.scopeCandidates(finding, effective).filter(option => option.broader)) {
      const directory = option.value.slice(0, -3);
      try { resolveSafeWorkspaceDirectory(root, directory); facts.directories.push(directory); }
      catch (error) { rejected.push(directory + ': ' + error.message); }
    }
    return { options: core.projectScopeOptions(finding, effective, facts), note: [note, ...rejected].filter(Boolean).join('\n') };
  }
  async function selectFinding(uri, reset = true) {
    const previousScope = fields.scope;
    const version = ++generation;
    file = uri.fsPath; scopes = []; fields.scope = ''; authority = {};
    const abort = new AbortController(); controller = abort; setState('running', '验证 Finding 与范围来源');
    try {
      const finding = await readFinding();
      const loaded = await loadScopes(finding, abort.signal);
      if (version !== generation || disposed) return;
      scopes = loaded.options; scopeNote = loaded.note; fields.scope = reset ? (scopes[0]?.value || '') : (previousScope || '');
      state = { phase: scopes.length ? 'ready' : 'failed', detail: scopes.length ? scopeNote : '无可靠 scope 候选；不会退回整仓库。' };
    } catch (error) { if (version === generation) state = { phase: abort.signal.aborted ? 'cancelled' : 'failed', detail: error.message }; }
    finally { controller = null; if (version === generation) publish(); else refresh(); }
  }
  async function outputPath(kind, inputs) {
    const picked = await vscode.window.showSaveDialog({ defaultUri: vscode.Uri.file(path.join(root, kind + '-request.json')), filters: { JSON: ['json'] }, saveLabel: '保存请求' });
    if (!picked) return null;
    resolveSafeSelectedPath(path.dirname(picked.fsPath), true);
    let previous = null;
    try { previous = await fs.stat(resolveSafeSelectedPath(picked.fsPath), { bigint: true }); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    for (const input of inputs) {
      const inputStat = await fs.stat(input, { bigint: true });
      if (path.resolve(input) === path.resolve(picked.fsPath) || (previous && previous.dev === inputStat.dev && previous.ino === inputStat.ino)) throw new Error('OUTPUT_OVERWRITES_INPUT');
    }
    if (previous && await vscode.window.showWarningMessage('覆盖已存在文件：' + picked.fsPath, { modal: true }, '覆盖') !== '覆盖') return null;
    return { path: picked.fsPath, modified: previous?.mtimeNs ?? null };
  }
  let view;
  let progress = '';
  let runSequence = 0;
  let controller;
  let disposed = false;
  let generation = 0;
  let state = { phase: 'ready', detail: '' };
  const history = [];
  const config = () => {
    const c = vscode.workspace.getConfiguration('erf', root ? vscode.Uri.file(root) : undefined);
    return { rfPath: c.get('rfPath', 'rf'), serverEndpoint: c.get('serverEndpoint', ''), connectionProfile: c.get('connectionProfile', ''), requestTimeoutMs: c.get('requestTimeoutMs', 5000) };
  };
  const publish = () => {
    if (disposed) return;
    events.fire();
    void vscode.commands.executeCommand('setContext', 'erf.running', !!controller);
    if (view) void view.webview.postMessage(viewState());
  };
  const setState = (phase, detail = '') => {
    if (['ready', 'unconfigured', 'running'].includes(phase)) progress = '';
    state = { phase, detail }; publish();
  };
  function refresh() {
    if (controller) return;
    const c = config();
    if (!root) return setState('unconfigured', '请选择工作区；不会默认取多根工作区的第一项。');
    if (!scene.fields) return setState('unavailable', '阶段 1 尚未提供此命令的参数面板；不执行模板或自动准备安装资产。\n' + scene.commandTemplate);
    if (!c.rfPath || (scene.server && !c.serverEndpoint) || (scene.registry && !c.connectionProfile && !c.serverEndpoint)) return setState('unconfigured', '请在 ERF 设置中显式配置 rf 路径 / Server endpoint 或 registry connectionProfile FILE。');
    if (!Number.isInteger(c.requestTimeoutMs) || c.requestTimeoutMs <= 0) return setState('unconfigured', 'requestTimeoutMs 必须为正整数。');
    setState('ready', 'rf 可用性将在显式执行时确认。提交不等于批准 / Gate PASS / SCM 写入。');
  }
  const tree = vscode.window.createTreeView('erf.scenarios', { treeDataProvider: {
    onDidChangeTreeData: events.event,
    getChildren(node) {
      if (!node) return [...Object.entries(commandMap.sections).map(([id, label]) => ({ id, label, group: true })), { id: 'runs', label: '运行记录', group: true }];
      if (node.id === 'runs') return history.map(run => ({ id: run.id, label: run.label + ' · ' + LABELS[run.phase], run }));
      return [...SCENARIOS.filter(s => s.section === node.id), ...commandMap.commands.filter(c => c.section === node.id && !SCENARIOS.some(s => s.cli === c.commandId)).map(c => ({ ...c, id: c.commandId, label: c.commandTemplate, unavailable: true }))];
    },
    getTreeItem(node) {
      const collapsed = ['pilot_governance', 'operations_status', 'runs'].includes(node.id);
      const item = new vscode.TreeItem(node.label, node.group ? (collapsed ? 1 : 2) : 0);
      item.id = node.id;
      item.iconPath = new vscode.ThemeIcon(node.group ? (node.id === 'runs' ? 'history' : 'folder') : node.run ? 'history' : sceneIcon(node));
      if (node.run) {
        item.command = { command: 'erf.showRun', title: '查看运行回执', arguments: [node.id] };
        item.description = node.run.timestamp;
        item.tooltip = node.run.label + '\n' + node.run.timestamp;
      }
      if (node.section) {
        item.command = { command: 'erf.selectScene', title: node.label, arguments: [node.id] };
        item.description = node.unavailable ? '不可用 · 待后续阶段' : scene.id === node.id ? LABELS[state.phase] : '';
      }
      return item;
    },
    getParent(node) { return node.section ? { id: node.section, label: commandMap.sections[node.section], group: true } : undefined; },
  } });
  async function open(id, object) {
    if (controller) { await vscode.commands.executeCommand('erf.details.focus'); return; }
    const next = SCENARIOS.find(s => s.id === id || s.cli === id) || commandMap.commands.find(c => c.commandId === id);
    if (!next) throw new Error('UNKNOWN_SCENE');
    scene = next.id ? next : { ...next, id: next.commandId, label: next.commandTemplate };
    fields = Object.fromEntries(Object.entries(fields).filter(([key]) => scene.fields?.includes(key)));
    if (scene.id === 'erf.draftFeedback' && !fields.classification) fields.classification = core.CLASSIFICATIONS[0];
    if (scene.id === 'erf.requestException' && !fields.scope) fields.scope = scopes[0]?.value || '';
    if (scene.registry) { fields.resource = registryType; if (scene.id === 'erf.registryList') fields.limit ||= '50'; }
    if (scene.server) { fields.request = selectedRequestPath || ''; fields.selector = 'requestId'; }
    if (scene.scan) { fields.route ||= 'rule-id'; fields.format ||= 'json'; fields.progress ||= 'text'; }
    if (scene.id === 'erf.configShow' || scene.id === 'erf.doctor') fields.format ||= 'text';
    generation++;
    refresh();
    await vscode.commands.executeCommand('workbench.view.extension.erf');
    await tree.reveal(scene, { select: true, focus: false });
    await vscode.commands.executeCommand('erf.details.focus');
    if (object?.fsPath && !scene.server && !scene.registry && !scene.scan && needsFindingScene(scene.id)) await selectFinding(object);
  }
  async function execute(input, cursor) {
    if (controller) return;
    validateMessage({ type: 'edit', fields: input }, scene);
    input = Object.fromEntries(Object.entries({ ...fields, ...input }).filter(([key]) => scene.fields.includes(key)));
    validateMessage({ type: 'run', fields: input }, scene);
    refresh();
    if (state.phase !== 'ready') return;
    const selected = scene;
    const c = config();
    const runRoot = root;
    if (selected.registry) {
      if (fields.pin !== input.pin) descriptor = '';
      if (input.resource !== registryType) { registryType = input.resource; registryPage = null; }
    }
    fields = { ...input };
    const abort = new AbortController();
    controller = abort;
    const version = generation;
    setState('running', selected.label);
    try {
      const finding = needsFindingScene(selected.id) ? await readFinding() : null;
      let pin;
      let saved;
      let output;
      let previousModified;
      if (abort.signal.aborted || generation !== version || disposed) return;
      let args;
      switch (selected.id) {
        case 'erf.registryList':
          args = core.buildRegistryListArgs(input.resource, { ...registryConnection(c), limit: Number(input.limit), cursor });
          break;
        case 'erf.registryShow': case 'erf.registrySync':
          pin = core.selectRegistryPin(releasePins, input.pin, input.resource);
          args = selected.id === 'erf.registryShow' ? core.buildRegistryShowArgs(pin, registryConnection(c))
            : core.buildRegistrySyncArgs(pin, descriptor || pin.deliveryDescriptor, runRoot, registryConnection(c));
          break;
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
        case 'erf.explainFinding': args = core.buildExplainArgs(file, { format: 'text', ...authority }); break;
        case 'erf.scan': case 'erf.doctor': case 'erf.configValidate': case 'erf.configShow':
        case 'erf.standardValidate': case 'erf.ruleValidate': {
          // 统一命令面：同一 map params 机械构造 argv（含 scan 路线互斥/base-head 成对校验）。
          const cmd = core.mapCommand(commandMap, selected.cli);
          if (!cmd) throw new Error('COMMAND_MAP_MISSING:' + selected.cli);
          const values = { ...input };
          if (selected.autoRepoRoot) {
            if (!runRoot) throw new Error('WORKSPACE_REQUIRED_FOR_REPO_ROOT');
            values.repoRoot = runRoot; // 来自工作区选择，不由用户键入。
          }
          args = core.buildMappedArgs(cmd, values);
          if (selected.preview) {
            const preview = `${selected.previewNote}\nCLI argv：${JSON.stringify(args)}\n执行权威在 rf CLI；工作台只构造与展示。`;
            if (await vscode.window.showWarningMessage(preview, { modal: true }, '确认执行') !== '确认执行') return setState('cancelled', '未执行；表单保留。');
            if (abort.signal.aborted || generation !== version || disposed) return;
          }
          break;
        }
        case 'erf.draftFeedback': case 'erf.requestException': {
          const kind = selected.id === 'erf.draftFeedback' ? 'feedback' : 'exception';
          if (kind === 'exception') {
            const loaded = await loadScopes(finding, abort.signal);
            scopes = loaded.options; scopeNote = loaded.note;
            core.selectScope(scopes, input.scope, true); // Membership first, human confirmation below.
          } else resolveSafeWorkspaceFile(runRoot, finding.location.path);
          // Validate required CLI fields before prompting or writing anything.
          const build = kind === 'feedback' ? core.buildFeedbackDraftArgs : core.buildExceptionRequestArgs;
          build(file, input);
          const chosenOutput = await outputPath(kind, [file, resolveSafeWorkspaceFile(runRoot, finding.location.path)]);
          if (!chosenOutput) return setState('cancelled', '未选择保存位置；表单保留，未创建申请。');
          output = chosenOutput.path; previousModified = chosenOutput.modified;
          args = build(file, { ...input, output });
          const expanded = kind === 'exception' && scopes.find(option => option.value === input.scope)?.broader;
          const preview = `${expanded ? '扩大范围，须明确确认\n' : ''}actor：${input.actor}\nscope：${input.scope || '不适用'}\n输出：${output}\nCLI argv：${JSON.stringify(args)}\n仅保存草稿/申请，非批准。`;
          if (await vscode.window.showWarningMessage(preview, { modal: true }, '确认保存') !== '确认保存') return setState('cancelled', '未确认身份/范围；表单保留，未创建申请。');
          if (kind === 'exception') core.selectScope(scopes, input.scope, true);
          break;
        }
        case 'erf.submitFeedback': case 'erf.submitException': case 'erf.submissionStatus': {
          saved = core.selectRequest(availableRequests(), input.request);
          await readRequest(saved);
          selectedRequestPath = saved.path;
          if (selected.id === 'erf.submissionStatus') args = core.buildSubmissionStatusArgs({ ...core.statusSelection(saved, input.selector), server: c.serverEndpoint, timeoutMs: c.requestTimeoutMs });
          else {
            const endpoint = new URL(c.serverEndpoint); endpoint.username = ''; endpoint.password = ''; endpoint.search = '';
            if (await vscode.window.showWarningMessage(`提交到 ${endpoint}\nrequestId：${saved.requestId}\n文件：${saved.path}\n仅发送请求契约字段，不发送源码/完整 Finding；仅接收，非批准。`, { modal: true }, '确认提交') !== '确认提交') return setState('cancelled', '未提交；已保存请求保留。');
            await readRequest(saved); // Confirmation dialogs can outlive a file edit.
            saved = { ...saved, endpointKey: checksum(c.serverEndpoint), status: 'UNKNOWN' };
            await remember(saved); // Preserve identity even if the transport times out.
            args = core.buildSubmitArgs(saved.kind, saved.path, { server: c.serverEndpoint, timeoutMs: c.requestTimeoutMs });
          }
          break;
        }
        default: throw new Error('UNAVAILABLE_SCENE');
      }
      if (abort.signal.aborted || generation !== version || disposed) return;
      if (selected.registry) {
        const preview = `CLI argv：${JSON.stringify(args)}\n独立发布来源：${pin?.provenance || '无（仅浏览）'}\n${selected.id === 'erf.registrySync' ? '将按外部签名 descriptor 写入摘要寻址缓存；不会覆盖为可编辑规则。' : '只读元数据，PIN_MATCH 不是安装或发布证明。'}`;
        if (await vscode.window.showWarningMessage(preview, { modal: true }, '确认执行') !== '确认执行') return setState('cancelled', '未执行；独立 pin 与表单保留。');
        if (abort.signal.aborted || generation !== version || disposed) return;
      }
      const result = await invoke(args, c, runRoot, abort.signal, { onStderrLine(line) {
        if (generation !== version || disposed || abort.signal.aborted) return;
        progress += (progress ? '\n' : '') + line;
        publish();
      } });
      if (generation !== version || disposed) return;
      if (result.reason === 'CANCELLED' || abort.signal.aborted) return setState('cancelled', '已停止；远端请求可能已经接收，取消不代表回滚，请查询状态。');
      if (result.reason === 'UNAVAILABLE') return setState('unavailable', result.stderr);
      if (selected.scan) {
        // 扫描工作台：exit 0/2/3/4 全部按既有 ScanResult/ScanFailure 契约渲染；
        // 绝不把 exit=0 当作通过，由结构化 status/gate/authority 决定展示。
        if (result.reason) return setState('failed', result.reason + '\n' + result.stderr);
        setState('completed', core.renderScanReport(core.parseScanStdout(result.stdout), result.code, result.stderr));
        return;
      }
      if (result.reason || core.classifyExitCode(result.code) !== 'OK') return setState('failed', `${selected.registry ? 'UNKNOWN · 未安装 · ' : selected.server ? 'UNKNOWN/PENDING · ' : ''}${result.reason || core.classifyExitCode(result.code)} · exit=${result.code}\n${result.stderr || result.stdout}`);
      let detail = result.stdout;
      if (selected.registry) {
        const receipt = JSON.parse(result.stdout);
        if (selected.id === 'erf.registryList') {
          registryPage = core.projectRegistryPage(receipt, input.resource, cursor ?? '');
          detail = receipt.digestVerification + ' · authority=UNKNOWN · 未安装 / 不代表可执行\n' + JSON.stringify(registryPage.items, null, 2);
        } else detail = selected.id === 'erf.registryShow' ? core.renderRegistryShow(receipt, pin) : core.renderRegistrySync(receipt, pin);
      }
      if (output) {
        const realPath = resolveSafeSelectedPath(output);
        const written = await fs.stat(realPath, { bigint: true });
        if (written.size === 0n || (previousModified !== null && written.mtimeNs === previousModified)) throw new Error('OUTPUT_NOT_WRITTEN');
        const content = await fs.readFile(realPath, 'utf8');
        const request = JSON.parse(content);
        const identity = core.projectSavedRequest(request);
        const expectedKind = selected.id === 'erf.draftFeedback' ? 'feedback' : 'exception';
        if (identity.kind !== expectedKind || request.ruleId !== finding.ruleId || request.findingFingerprint !== finding.fingerprint
          || request.actor !== input.actor || request.reason !== input.reason
          || request[expectedKind === 'feedback' ? 'classification' : 'scope'] !== input[expectedKind === 'feedback' ? 'classification' : 'scope']) throw new Error('SAVED_REQUEST_MISMATCH');
        const record = { ...identity, path: realPath, root: runRoot, endpointKey: null, contentDigest: checksum(content), savedAt: new Date().toISOString() };
        try { await remember(record); }
        catch (error) { throw new Error(`已写入 ${realPath}，但请求索引保存失败：${error.message}`); }
        selectedRequestPath = realPath;
        detail = core.statusLabel(identity.status) + '\n已保存：' + realPath + '\nrequestId：' + identity.requestId;
      }
      if (selected.server) {
        const receipt = core.validateSubmissionResponse(JSON.parse(result.stdout), saved, selected.id === 'erf.submissionStatus');
        const rendered = core.renderReceiptGuards(receipt);
        if (!rendered.guardsHold) throw new Error('回执违反权威守卫：' + rendered.violations.join(','));
        const receiptIndex = { status: receipt.status, requestId: saved.requestId, artifactDigest: saved.artifactDigest, serverRequestId: receipt.serverRequestId };
        await remember({ ...saved, endpointKey: checksum(c.serverEndpoint), status: receipt.status, receipt: receiptIndex });
        detail = rendered.status + '\n' + rendered.note + '\n' + result.stdout;
      }
      setState('completed', 'exit=0\n' + detail);
    } catch (error) {
      if (generation === version && !disposed) setState(abort.signal.aborted ? 'cancelled' : 'failed', error.message);
    } finally {
      controller = null;
      if (generation === version && !disposed) {
        if (abort.signal.aborted) state = { phase: 'cancelled', detail: '已取消；远端副作用未知，必要时查询回执。' };
        history.unshift(Object.freeze({ id: 'run-' + (++runSequence), label: selected.label, timestamp: new Date().toISOString(), phase: state.phase, detail: state.detail }));
        history.splice(20);
        publish();
      } else refresh();
    }
  }
  async function message(raw) {
    try {
      const m = validateMessage(raw, scene);
      if (m.type === 'ready') { publish(); return; }
      if (m.type === 'cancel') { controller?.abort(); return; }
      if (controller) return;
      if (m.type === 'edit') {
        if (scene.registry) {
          if (m.fields.resource && m.fields.resource !== registryType) {
            registryType = m.fields.resource; registryPage = null; descriptor = '';
            fields = { ...m.fields, pin: '' }; publish(); return;
          }
          if (m.fields.limit !== fields.limit) registryPage = null;
          if (m.fields.pin !== fields.pin) descriptor = '';
        }
        fields = { ...m.fields };
        if (Object.hasOwn(fields, 'request')) selectedRequestPath = fields.request ? core.selectRequest(availableRequests(), fields.request).path : null;
        if (scene.registry) publish(); // Refresh descriptor/provenance when the whole pin selection changes.
        return;
      }
      if (m.type === 'run') return await execute(m.fields);
      if (m.type === 'refresh') { if (scene.registry) { registryPage = null; return refresh(); } if (file && !scene.server) return selectFinding(vscode.Uri.file(file), false); return refresh(); }
      if (m.type === 'settings') return await vscode.commands.executeCommand('workbench.action.openSettings', 'erf');
      if (m.type === 'clearAuthority') { authority = {}; publish(); return; }
      const version = generation;
      if (m.type === 'nextPage') {
        if (scene.id !== 'erf.registryList' || !registryPage?.hasMore) throw new Error('REGISTRY_NO_NEXT_PAGE');
        return await execute(fields, registryPage.nextCursor);
      }
      if (m.type === 'pickReleasePins' || m.type === 'pickDescriptor') {
        if (!scene.registry) throw new Error('INVALID_REGISTRY_ACTION');
        const result = await vscode.window.showOpenDialog({ canSelectMany: false, title: m.type === 'pickReleasePins' ? '选择独立发布记录 JSON（不是信任根）' : '选择外部签名 BundleDeliveryDescriptor JSON', filters: { JSON: ['json'] } });
        if (!result?.length || version !== generation || controller) return;
        const selectedPath = resolveSafeSelectedPath(result[0].fsPath);
        if (m.type === 'pickReleasePins') {
          const pins = core.projectRegistryPins(JSON.parse(await fs.readFile(selectedPath, 'utf8')));
          if (version !== generation || controller) return;
          releasePins = pins.map(pin => Object.freeze({ ...pin, ...(pin.deliveryDescriptor ? { deliveryDescriptor: path.resolve(path.dirname(selectedPath), pin.deliveryDescriptor) } : {}) }));
          releaseSource = selectedPath; fields.pin = ''; descriptor = '';
        } else descriptor = selectedPath;
        publish(); return;
      }
      if (m.type === 'pickAuthority') {
        const result = await vscode.window.showOpenDialog({ canSelectMany: false, title: '选择 ExecutionResult JSON', filters: { JSON: ['json'] } });
        if (!result?.length || generation !== version) return;
        const trust = await vscode.window.showOpenDialog({ canSelectMany: false, title: '选择独立 AuthorityTrust JSON', filters: { JSON: ['json'] } });
        if (trust?.length && generation === version) { authority = { result: result[0].fsPath, authorityTrust: trust[0].fsPath }; publish(); }
      } else if (m.type === 'pickFile' && needsFindingScene(scene.id)) {
        const picked = await vscode.window.showOpenDialog({ canSelectMany: false, filters: { JSON: ['json'] }, defaultUri: root ? vscode.Uri.file(root) : undefined });
        if (picked?.length && generation === version) await selectFinding(picked[0]);
      } else if (m.type === 'workspace') {
        const picked = await vscode.window.showWorkspaceFolderPick();
        if (picked && generation === version) { root = picked.uri.fsPath; clearSelection(); generation++; refresh(); }
      }
    } catch (error) { if (!controller) setState('failed', error.message); }
  }
  function viewState() {
    const busy = !!controller;
    const labels = { reason: '理由（多行）', actor: 'actor（保存前须本人确认）', classification: '分类（本地 CLI 契约）', scope: 'scope（实际范围）', request: '当前已保存请求', selector: '查询选择器', resource: 'registry 类型', limit: '每页条数', pin: '独立发布记录：身份 / 版本 / 摘要 / provenance（完整绑定）', route: '扫描路线（互斥）', ruleId: 'rule id（route=rule-id）', candidate: 'candidate 目录（route=candidate）', deliveryDescriptor: 'delivery descriptor（route=delivery-descriptor）', format: '输出格式', progress: '进度输出', output: '结果/回执输出文件', deadlineMs: 'deadline 毫秒（缺省 600 秒）', base: 'base revision（与 head 成对）', head: 'head revision（与 base 成对）', host: 'host 命令（高级覆盖）', resolver: 'resolver 命令（高级覆盖）', runtimeHome: 'runtime home（高级覆盖）', config: '工作区配置文件（缺省 .ruleops/config.yaml）', scanProbe: 'scan-probe 外部命令（显式确认才执行）', files: '待校验文件（逗号分隔路径）' };
    const choices = {
      resource: (scene.id === 'erf.registrySync' ? ['rules'] : ['rules', 'standards']).map(value => ({ value, label: value })),
      limit: ['10', '50', '100', '256'].map(value => ({ value, label: value })),
      pin: releasePins.flatMap((pin, index) => pin.resource === fields.resource ? [{ value: String(index), label: `${pin.id} @ ${pin.version} · ${pin.expectedDigest} · ${pin.provenance}` }] : []),
      classification: core.CLASSIFICATIONS.map(value => ({ value, label: value })),
      scope: scopes.map(o => ({ value: o.value, label: `${o.value} · ${o.source} · 匹配当前文件${o.broader ? ' · 扩大范围需确认' : ''}` })),
      request: availableRequests().map(r => ({ value: r.path, label: `${r.kind} · ${r.savedAt} · ${r.status} · ${r.path} · ${r.requestId}` })),
      selector: ['requestId', 'artifactDigest'].map(value => ({ value, label: value })),
      route: [{ value: 'rule-id', label: 'rule-id（规则身份）' }, { value: 'candidate', label: 'candidate（本地候选）' }, { value: 'delivery-descriptor', label: 'delivery-descriptor（发布描述符）' }],
      format: (scene.scan ? ['text', 'json', 'sarif'] : ['text', 'json']).map(value => ({ value, label: value })),
      progress: ['text', 'json', 'none'].map(value => ({ value, label: value })),
    };
    const needsFile = needsFindingScene(scene.id);
    const canRun = !busy && !!scene.fields && !!(scene.registry || scene.scan || (scene.server ? choices.request.length : !needsFile || file))
      && ['ready', 'failed', 'completed', 'cancelled'].includes(state.phase);
    const validChoices = (scene.fields || []).every(name => !choices[name] || choices[name].some(choice => choice.value === fields[name]));
    const params = core.mapCommand(commandMap, scene.cli)?.params || [];
    const controls = (scene.fields || []).map(name => {
      const value = fields[name] || '';
      const available = choices[name];
      // The shared deadline label still claims no cutoff; the local process has a 600s budget.
      return { name, value, label: name === 'deadlineMs' ? labels[name] : params.find(param => param.name === name)?.label || labels[name],
        required: ['actor', 'reason'].includes(name) || !!available,
        ...(available ? { choices: [{ value: '', label: '请选择' },
          ...(value && !available.some(choice => choice.value === value) ? [{ value, label: '已失效：' + value, invalid: true }] : []), ...available] } : {}) };
    });
    const c = config();
    return { type: 'update', contextKey: `${scene.id}:${root}:${generation}`, title: scene.label, status: LABELS[state.phase],
      detail: state.detail, busy, progress, controls, canRun, validChoices, hasMore: !!registryPage?.hasMore,
      workspace: (root || '未选择工作区') + '\nServer：' + (c.serverEndpoint ? '已配置（连接由 CLI 校验）' : '未配置'),
      file: file || '未选择文件', authority: (authority.result || '无权威配对') + ' ' + (authority.authorityTrust || ''),
      scopeNote: scopeNote || (scopes.length ? '默认当前文件；扩范围须确认；预览不授予例外权威。' : '无可靠 scope 候选，请选择有效 Finding。'),
      registryNote: '连接：' + (c.connectionProfile ? 'profile FILE：' + c.connectionProfile + '（优先于 endpoint；失败不回退）' : '显式 Server endpoint（由 CLI 校验）') + '\nFORMAT_ONLY / PIN_MATCH · authority=UNKNOWN；不是安装。',
      releaseNote: releaseSource || '缺少独立发布来源，不能核对/同步',
      descriptorNote: descriptor || releasePins.find((_, index) => String(index) === fields.pin)?.deliveryDescriptor || '缺少 descriptor；禁止从响应构造',
      pageNote: registryPage ? '当前页 ' + registryPage.items.length + ' 条 · UNKNOWN' : '尚未浏览',
      executeLabel: scene.id === 'erf.registryList' ? '读取/刷新第一页' : scene.id === 'erf.registryShow' ? '核对独立发布 pin'
        : scene.id === 'erf.registrySync' ? '确认缓存签名包' : ['erf.draftFeedback', 'erf.requestException'].includes(scene.id) ? '预览并保存草稿/申请' : '执行所选场景',
      visible: { 'finding-section': needsFile, 'authority-section': scene.id === 'erf.explainFinding', 'scope-note': scene.id === 'erf.requestException',
        'registry-section': !!scene.registry, 'descriptor-section': scene.id === 'erf.registrySync', 'page-section': scene.id === 'erf.registryList' } };
  }
  const register = (id, fn) => context.subscriptions.push(vscode.commands.registerCommand(id, fn));
  register('erf.selectScene', open);
  register('erf.showRun', async id => {
    if (controller) return;
    const run = history.find(entry => entry.id === id);
    if (!run) throw new Error('UNKNOWN_RUN');
    progress = '';
    setState(run.phase, `运行记录：${run.label}\n时间：${run.timestamp}\n${run.detail}`);
    await vscode.commands.executeCommand('erf.details.focus');
  });
  register('erf.refresh', refresh);
  register('erf.cancel', () => controller?.abort());
  register('erf.settings', () => vscode.commands.executeCommand('workbench.action.openSettings', 'erf'));
  context.subscriptions.push(tree, events, vscode.window.registerWebviewViewProvider('erf.details', {
    resolveWebviewView(resolved) {
      view = resolved;
      view.webview.options = { enableScripts: true, localResourceRoots: [] };
      view.webview.html = html(randomBytes(18).toString('base64'));
      context.subscriptions.push(view.webview.onDidReceiveMessage(message), view.onDidDispose(() => { view = undefined; }));
      publish();
    },
  }), vscode.workspace.onDidChangeConfiguration(event => {
    if (!event.affectsConfiguration('erf')) return;
    generation++; controller?.abort(); clearRegistry(); selectedRequestPath = null; fields = {}; scopes = []; authority = {}; refresh();
  }), vscode.workspace.onDidChangeWorkspaceFolders(() => {
    if (root && !vscode.workspace.workspaceFolders?.some(f => f.uri.fsPath === root)) {
      generation++; controller?.abort(); root = null; clearSelection(); refresh();
    }
  }), { dispose() { disposed = true; generation++; controller?.abort(); } });
  refresh();
  return { open, execute, message, get state() { return state; }, get requests() { return availableRequests(); } };
}
module.exports = { createSidebar, SCENARIOS, validateMessage };
