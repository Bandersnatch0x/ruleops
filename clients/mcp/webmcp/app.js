'use strict';
const $ = (id) => document.getElementById(id);
const context = document.modelContext;
const registered = new Map();
// Keep these offline definitions identical to the server whitelist (checked by app.test.js).
const text = (description) => ({ type: 'string', minLength: 1, description });
const root = { repo_root: text('Repository path, relative to the server working directory; default .') };
const builtinTools = [
  {
    name: 'doctor', description: 'Read rf environment/configuration diagnostics; never a Gate decision.',
    inputSchema: { type: 'object', properties: root, additionalProperties: false },
  },
  {
    name: 'config_show', description: 'Read effective rf project configuration and identity digests.',
    inputSchema: { type: 'object', properties: root, additionalProperties: false },
  },
  {
    name: 'scan', description: 'Run rf scan and return its digest-only projection without changing its decision.',
    inputSchema: {
      type: 'object', additionalProperties: false,
      properties: { ...root, rule_id: text('Rule identifier'), candidate: text('Existing Candidate directory') },
      oneOf: [{ required: ['rule_id'] }, { required: ['candidate'] }],
    },
  },
  {
    name: 'explain', description: 'Explain an existing digest-only FindingView; no authority is added.',
    inputSchema: {
      type: 'object', additionalProperties: false, required: ['finding'],
      properties: { ...root, finding: text('FindingView JSON file'), rule_meta: text('Existing rule metadata JSON file') },
    },
  },
].map((tool) => ({ ...tool, annotations: { readOnlyHint: true } }));
let tools = builtinTools;
function status(message, error = false) {
  $('status').textContent = message;
  $('status').className = error ? 'error' : '';
}
function busy(active) {
  $('run').disabled = active;
  $('run').textContent = active ? '正在调用…' : '调用并查看';
  $('tool').disabled = active;
  $('form').ariaBusy = String(active);
  $('results').ariaBusy = String(active);
  $('progress').hidden = !active;
}
function render(result) {
  $('json').textContent = JSON.stringify(result, null, 2);
  const value = result.structuredContent || result;
  const container = $('summary');
  container.replaceChildren();
  const headline = document.createElement('p');
  headline.className = 'result-meta data';
  headline.textContent = `isError: ${result.isError ?? '未知'} · rc: ${result._meta?.['erf/exitCode'] ?? '未知'} · ${value.kind || 'JSON'}（非新增权威）`;
  container.append(headline);
  if (value.kind === 'ScanResult') {
    const facts = document.createElement('dl');
    for (const [label, data] of [
      ['扫描判定 · gateDecision', value.gateDecision ?? '未提供'],
      ['权威来源 · authority（仅转述 CLI）', value.authority ?? '未提供'],
      ['发现记录 · findingRecords', `${value.findingRecords?.length ?? 0} 条`],
    ]) {
      const dt = document.createElement('dt'); dt.textContent = label;
      const dd = document.createElement('dd'); dd.className = 'data';
      dd.textContent = typeof data === 'string' ? data : JSON.stringify(data, null, 2);
      facts.append(dt); facts.append(dd);
    }
    container.append(facts);
    return;
  }
  if (Array.isArray(value.checks)) {
    if (!value.checks.length) {
      const empty = document.createElement('p');
      empty.textContent = 'CLI 未返回检查项；这不代表检查通过。请展开原始 JSON 核对。';
      container.append(empty);
      return;
    }
    const columns = [['code', '检查项'], ['status', '状态'], ['detail', '说明'], ['nextStep', '下一步']];
    const table = document.createElement('table');
    table.createCaption().textContent = `环境诊断 · ${value.checks.length} 项检查（非 Gate 判定）`;
    const row = table.createTHead().insertRow();
    for (const [, label] of columns) {
      const th = document.createElement('th'); th.scope = 'col'; th.textContent = label; row.append(th);
    }
    const body = table.createTBody();
    const reading = document.createElement('details');
    const title = document.createElement('summary'); title.textContent = '逐项阅读（表格替代）';
    reading.append(title);
    for (const check of value.checks) {
      const row = body.insertRow();
      const list = document.createElement('dl');
      for (const [key, label] of columns) {
        const content = typeof check[key] === 'string' ? check[key] : JSON.stringify(check[key] ?? '—');
        row.insertCell().textContent = content;
        const dt = document.createElement('dt'); dt.textContent = label;
        const dd = document.createElement('dd'); dd.textContent = content;
        list.append(dt); list.append(dd);
      }
      reading.append(list);
    }
    container.append(table); container.append(reading);
  } else {
    const list = document.createElement('dl');
    for (const [key, data] of Object.entries(value)) {
      const dt = document.createElement('dt'); dt.textContent = key;
      const dd = document.createElement('dd'); dd.className = 'data';
      dd.textContent = typeof data === 'string' ? data : JSON.stringify(data, null, 2);
      list.append(dt); list.append(dd);
    }
    container.append(list);
  }
}
async function execute(tool, args) {
  busy(true);
  status(`正在调用 ${tool}，等待 CLI 返回…`);
  $('summary').textContent = '正在加载结果，请稍候；完成前不作任何判定。';
  $('json').textContent = '正在等待本次调用结果…';
  let result;
  try {
    const response = await fetch('/api/rf', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tool, args }) });
    result = await response.json();
    render(result);
    if (!response.ok) throw new Error(`HTTP ${response.status}: ${JSON.stringify(result.error)}`);
    status(`${tool} 已完成；isError=${result.isError}，rc=${result._meta?.['erf/exitCode'] ?? '未知'}。${result.isError ? '请按结果中的下一步修复后重新调用。' : '仅展示 CLI 判定。'}`, result.isError);
    return result;
  } catch (error) {
    if (!result) $('json').textContent = '本次未收到有效 JSON，请检查错误信息后重试。';
    $('summary').textContent = `调用失败：${error.message}。请检查本地服务及参数，再点击「调用并查看」重试。`;
    status(`调用失败：${error.message}。请检查本地服务及参数后重试。`, true);
    throw error;
  } finally { busy(false); }
}
const fieldCopy = {
  repo_root: ['仓库目录', '相对路径以服务启动目录为基准；留空使用该目录。'],
  rule_id: ['规则标识', '与 Candidate 目录恰好填写一项。'],
  candidate: ['Candidate 目录', '填写已有目录；与规则标识恰好填写一项。'],
  finding: ['FindingView 文件', '填写已有的 digest-only FindingView JSON 文件路径。'],
  rule_meta: ['规则元数据文件', '可选；填写已有规则元数据 JSON 文件路径。'],
};
const toolCopy = {
  doctor: '诊断本地环境与配置；诊断成功不代表 Gate PASS。',
  config_show: '查看生效配置与身份摘要，不修改配置。',
  scan: '运行扫描并展示 digest-only 结果，不改变 CLI 原始判定。',
  explain: '解释已有 FindingView，不增加任何权威。',
};
function validateFields() {
  const inputs = [...$('fields').querySelectorAll('input')];
  const args = Object.fromEntries(inputs.map((input) => [input.name, input.value]));
  let valid = true;
  for (const input of inputs) {
    let message = '';
    if (input.required && !input.value.trim()) message = '此项必填，请填写已有文件路径。';
    else if (input.value && (!input.value.trim() || input.value.startsWith('-') || /[\x00-\x1f\x7f]/.test(input.value))) {
      message = '参数不能仅为空白、以 - 开头或包含控制字符，请修改后重试。';
    }
    if ($('tool').value === 'scan' && ['rule_id', 'candidate'].includes(input.name) && Boolean(args.rule_id) === Boolean(args.candidate)) {
      message = '规则标识与 Candidate 目录必须恰好填写一项：填写其中一项，并清空另一项。';
    }
    input.ariaInvalid = String(Boolean(message));
    $(input.id + '-error').textContent = message;
    input.ariaDescription = `${$(input.id + '-hint').textContent} ${message}`;
    valid = valid && !message;
  }
  return valid;
}
function fields() {
  const tool = tools.find((item) => item.name === $('tool').value);
  $('description').textContent = toolCopy[tool.name];
  $('fields').replaceChildren();
  for (const key of Object.keys(tool.inputSchema.properties)) {
    const group = document.createElement('div'); group.className = 'field';
    const label = document.createElement('label'); label.htmlFor = `field-${key}`;
    label.textContent = fieldCopy[key][0];
    const optional = document.createElement('span');
    optional.textContent = tool.inputSchema.required?.includes(key) ? '（必填）' : '（可选）';
    label.append(optional);
    const input = document.createElement('input');
    input.id = label.htmlFor; input.name = key;
    input.required = tool.inputSchema.required?.includes(key) || false;
    input.autocomplete = 'off'; input.spellcheck = false;
    input.addEventListener('blur', validateFields);
    const hint = document.createElement('p'); hint.id = `${input.id}-hint`; hint.className = 'hint';
    hint.textContent = `${key} · ${fieldCopy[key][1]}`;
    const error = document.createElement('p'); error.id = `${input.id}-error`; error.className = 'field-error';
    input.ariaDescription = hint.textContent;
    error.ariaLive = 'polite';
    group.append(label); group.append(input); group.append(hint); group.append(error); $('fields').append(group);
  }
}
$('form').addEventListener('submit', async (event) => {
  event.preventDefault();
  if (!validateFields()) {
    status('参数尚未填写正确；请按字段下方提示修复。', true);
    $('fields').querySelector('[aria-invalid="true"]').focus();
    return;
  }
  const name = $('tool').value;
  const args = Object.fromEntries([...new FormData($('form'))].filter(([, value]) => value !== ''));
  busy(true);
  status(`正在调用 ${name}…`);
  try {
    if (registered.has(name)) {
      if (typeof context.getTools === 'function' && typeof context.executeTool === 'function') {
        const available = await context.getTools();
        const tool = available.find((item) => item.name === name && item.origin === location.origin && item.window === window);
        if (!tool) throw new Error(`WebMCP 工具不可发现：${name}`);
        await context.executeTool(tool, JSON.stringify(args));
      } else {
        // Early experimental implementations may only expose registration.
        await registered.get(name).execute(args);
      }
    } else await execute(name, args);
  } catch (error) { status(`调用失败：${error.message}。请检查本地服务及参数后重试。`, true); }
  finally { busy(false); }
});
$('tool').addEventListener('change', fields);
function refreshTools() {
  const selected = $('tool').value;
  $('tool').replaceChildren();
  for (const tool of tools) {
    const option = document.createElement('option'); option.value = tool.name; option.textContent = tool.name;
    $('tool').append(option);
  }
  if (tools.some((tool) => tool.name === selected)) $('tool').value = selected;
  fields();
}
async function refreshManualTools() {
  try {
    const response = await fetch('/api/tools', { signal: AbortSignal.timeout(10_000) });
    if (!response.ok) throw new Error(`工具加载失败：HTTP ${response.status}`);
    ({ tools } = await response.json());
    refreshTools();
  } catch (error) {
    status(`手动工具列表刷新失败；保留内置工具。${error.message}`, true);
  }
}
function init() {
  const supported = typeof context?.registerTool === 'function';
  $('webmcp').textContent = 'WebMCP: 未启用（此浏览器不支持，仍可手动查看）';
  if (supported) {
    try {
      for (const tool of builtinTools) {
        const definition = { ...tool, execute: (args) => execute(tool.name, args) };
        context.registerTool(definition); registered.set(tool.name, definition);
      }
      $('webmcp').textContent = 'WebMCP: 已启用（Chrome 146+）';
    } catch (error) {
      // Keep successful registrations active; failed registrations use manual HTTP.
      $('webmcp').textContent = `WebMCP: 注册未全部完成（${registered.size}/4）；仍可手动查看。${error.message}`;
    }
  }
  refreshTools(); $('tool').disabled = false; $('run').disabled = false; status('就绪；仅展示 CLI 原始判定。');
  void refreshManualTools();
}
init();
