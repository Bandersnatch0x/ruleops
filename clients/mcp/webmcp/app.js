'use strict';
const $ = (id) => document.getElementById(id);
const context = document.modelContext;
const registered = new Map();
let tools = [];
function status(message, error = false) {
  $('status').textContent = message;
  $('status').className = error ? 'error' : '';
}
function render(result) {
  $('json').textContent = JSON.stringify(result, null, 2);
  const value = result.structuredContent || result;
  const container = $('summary');
  container.replaceChildren();
  const headline = document.createElement('p');
  headline.textContent = `isError: ${result.isError ?? '未知'} · rc: ${result._meta?.['erf/exitCode'] ?? '未知'} · ${value.kind || 'JSON'}（非新增权威）`;
  container.append(headline);
  if (value.kind === 'ScanResult') {
    const scan = document.createElement('p');
    scan.textContent = `扫描判定：${value.gateDecision ?? '未提供'} · authority：${JSON.stringify(value.authority ?? '未提供')} · findingRecords：${value.findingRecords?.length ?? 0} 条（仅转述 CLI）`;
    container.append(scan);
  }
  const checks = value.checks;
  if (Array.isArray(checks)) {
    const wrapper = document.createElement('div');
    wrapper.className = 'table-wrap';
    const table = document.createElement('table');
    const row = table.insertRow();
    for (const key of ['code', 'status', 'detail', 'nextStep']) {
      const th = document.createElement('th'); th.scope = 'col'; th.textContent = key; row.append(th);
    }
    for (const check of checks) {
      const row = table.insertRow();
      for (const key of ['code', 'status', 'detail', 'nextStep']) row.insertCell().textContent =
        typeof check[key] === 'string' ? check[key] : JSON.stringify(check[key] ?? '—');
    }
    wrapper.append(table); container.append(wrapper);
  } else {
    const summary = document.createElement('pre');
    summary.textContent = JSON.stringify(value.kind === 'ScanResult' ? {
      status: value.status, gateDecision: value.gateDecision, authority: value.authority,
      findingRecords: value.findingRecords, completeness: value.completeness, diagnostics: value.diagnostics,
    } : value, null, 2);
    container.append(summary);
  }
}
async function execute(tool, args) {
  status(`正在调用 ${tool}…`);
  try {
    const response = await fetch('/api/rf', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tool, args }) });
    const result = await response.json();
    render(result);
    if (!response.ok) throw new Error(`HTTP ${response.status}: ${JSON.stringify(result.error)}`);
    status(`${tool} 已完成；isError=${result.isError}，rc=${result._meta['erf/exitCode']}。`, result.isError);
    return result;
  } catch (error) { status(error.message, true); throw error; }
}
function fields() {
  const tool = tools.find((item) => item.name === $('tool').value);
  $('description').textContent = tool.description + (tool.name === 'scan' ? ' · rule_id 与 candidate 必须恰选一项。' : '');
  $('fields').replaceChildren();
  for (const [key, schema] of Object.entries(tool.inputSchema.properties)) {
    const label = document.createElement('label');
    label.textContent = `${key}${tool.inputSchema.required?.includes(key) ? '（必填）' : '（可选）'}`;
    const input = document.createElement('input');
    input.name = key; input.required = tool.inputSchema.required?.includes(key) || false;
    input.placeholder = key === 'repo_root' ? '默认：服务启动目录' : schema.description;
    label.append(input); $('fields').append(label);
  }
}
$('form').addEventListener('submit', async (event) => {
  event.preventDefault(); $('run').disabled = true;
  try {
    const name = $('tool').value;
    const args = Object.fromEntries([...new FormData($('form'))].filter(([, value]) => value !== ''));
    if (registered.has(name)) {
      if (typeof context.getTools === 'function' && typeof context.executeTool === 'function') {
        const available = await context.getTools();
        const tool = available.find((item) => item.name === name && item.origin === location.origin && item.window === window);
        if (!tool) throw new Error(`WebMCP 工具不可发现：${name}`);
        await context.executeTool(tool, args);
      } else {
        // Early experimental implementations may only expose registration.
        await registered.get(name).execute(args);
      }
    } else await execute(name, args);
  } catch (error) { status(error.message, true); }
  finally { $('run').disabled = false; }
});
$('tool').addEventListener('change', fields);
async function init() {
  const supported = typeof context?.registerTool === 'function';
  $('webmcp').textContent = supported ? 'WebMCP: 正在注册…' : 'WebMCP: 未启用（此浏览器不支持，仍可手动查看）';
  const response = await fetch('/api/tools');
  if (!response.ok) throw new Error(`工具加载失败：HTTP ${response.status}`);
  ({ tools } = await response.json());
  for (const tool of tools) {
    const option = document.createElement('option'); option.value = tool.name; option.textContent = tool.name;
    $('tool').append(option);
  }
  if (supported) {
    try {
      for (const tool of tools) {
        const definition = { ...tool, annotations: { readOnlyHint: true }, execute: (args) => execute(tool.name, args) };
        await context.registerTool(definition); registered.set(tool.name, definition);
      }
      $('webmcp').textContent = 'WebMCP: 已启用（Chrome 146+）';
    } catch (error) {
      // Keep successful registrations active; failed registrations use manual HTTP.
      $('webmcp').textContent = `WebMCP: 注册未全部完成（${registered.size}/4）；仍可手动查看。${error.message}`;
    }
  }
  fields(); $('tool').disabled = false; $('run').disabled = false; status('就绪；仅展示 CLI 原始判定。');
}
init().catch((error) => status(error.message, true));
