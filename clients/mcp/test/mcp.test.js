'use strict';

const assert = require('node:assert/strict');
const { spawn, spawnSync } = require('node:child_process');
const { once } = require('node:events');
const http = require('node:http');
const path = require('node:path');
const readline = require('node:readline');
const test = require('node:test');
const { buildArgs, validateOutput, dispatch, options, tools } = require('../erf-mcp-server');

const ROOT = path.resolve(__dirname, '../../..');
const SERVER = path.resolve(__dirname, '../erf-mcp-server.js');
const NAMES = ['rf_doctor', 'rf_config_show', 'rf_scan', 'rf_explain'];
const DISABLED = ['rf_rule_generate', 'rf_rule_test', 'rf_coverage_analyze', 'rf_pilot_window_report',
  'rf_feedback_submit', 'rf_exception_submit', 'rf_pilot_evidence_package'];
const initialize = { protocolVersion: '2025-06-18', capabilities: {},
  clientInfo: { name: 'erf-mcp-test', version: '1.0.0' } };
const rpc = (id, method, params = {}) => ({ jsonrpc: '2.0', id, method, params });
const headers = { 'Content-Type': 'application/json', Accept: 'application/json, text/event-stream',
  'MCP-Protocol-Version': '2025-06-18' };

async function start(t, args = []) {
  const child = spawn(process.execPath, [SERVER, ...args], { cwd: ROOT, stdio: ['pipe', 'pipe', 'pipe'] });
  t.after(async () => {
    if (child.exitCode !== null || child.signalCode !== null) return;
    const exited = once(child, 'exit');
    child.kill('SIGKILL');
    await exited;
  });
  const stdout = readline.createInterface({ input: child.stdout });
  const stderr = readline.createInterface({ input: child.stderr });
  t.after(() => { stdout.close(); stderr.close(); });
  const line = (stream) => once(stream, 'line', { signal: AbortSignal.timeout(55_000) }).then(([value]) => value);
  return { child, stdout, stderr, line,
    async request(value) {
      const response = line(stdout);
      child.stdin.write((typeof value === 'string' ? value : JSON.stringify(value)) + '\n');
      return JSON.parse(await response);
    } };
}

function checkInitialize(result) {
  assert.equal(result.jsonrpc, '2.0');
  assert.equal(result.id, 1);
  assert.equal(result.result.protocolVersion, '2025-06-18');
  assert.deepEqual(result.result.capabilities, { tools: { listChanged: false } });
  assert.equal(result.result.serverInfo.name, 'erf-mcp-server');
  assert.equal(typeof result.result.serverInfo.version, 'string');
}

function checkTools(result) {
  assert.deepEqual(result.result.tools.map((entry) => entry.name), NAMES);
  for (const name of DISABLED) assert.ok(!result.result.tools.some((entry) => entry.name === name));
  for (const tool of result.result.tools) {
    assert.equal(tool.inputSchema.type, 'object');
    assert.equal(tool.inputSchema.additionalProperties, false);
    for (const key of ['output', 'host', 'resolver', 'scan_probe', 'format', 'server']) {
      assert.ok(!Object.hasOwn(tool.inputSchema.properties, key));
    }
  }
}

function checkDoctor(response) {
  assert.equal(response.jsonrpc, '2.0');
  assert.equal(response.id, 3);
  const result = response.result;
  const value = JSON.parse(result.content[0].text);
  assert.equal(result.content[0].type, 'text');
  assert.deepEqual(value, result.structuredContent);
  assert.equal(value.kind, 'DoctorReport');
  assert.equal(value.api_version, 'rulefactory.io/v1');
  assert.ok(Array.isArray(value.checks));
  const direct = spawnSync(path.join(ROOT, 'rf'), ['doctor', '--repo-root', ROOT, '--format', 'json'],
    { cwd: ROOT, encoding: 'utf8', timeout: 50_000 });
  assert.ifError(direct.error);
  assert.ok(Number.isInteger(direct.status));
  assert.deepEqual(value, JSON.parse(direct.stdout));
  assert.equal(result.isError, direct.status !== 0);
  assert.equal(result._meta['erf/exitCode'], direct.status);
}

test('real stdio: initialize, tools/list, rf_doctor; original diagnostics and rc survive', { timeout: 60_000 }, async (t) => {
  const server = await start(t);
  checkInitialize(await server.request(rpc(1, 'initialize', initialize)));
  server.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
  checkTools(await server.request(rpc(2, 'tools/list')));
  checkDoctor(await server.request(rpc(3, 'tools/call', { name: 'rf_doctor', arguments: { repo_root: '.' } })));
  const hostile = '$(touch MCP_INJECTION_SENTINEL)';
  const literal = await server.request(rpc(6, 'tools/call', { name: 'rf_doctor', arguments: { repo_root: hostile } }));
  assert.equal(literal.result.structuredContent.repository, path.resolve(ROOT, hostile));
  assert.equal(literal.result.isError, true);
  const missing = path.join(ROOT, '.scratch/mcp-server/missing-config-' + process.pid);
  const config = await server.request(rpc(7, 'tools/call', { name: 'rf_config_show', arguments: { repo_root: missing } }));
  assert.equal(config.result.isError, true);
  assert.equal(config.result.structuredContent.error, 'RF_INVALID_JSON');
  assert.match(config.result.structuredContent.stderrDigest, /^sha256:[a-f0-9]{64}$/);
  assert.ok(!JSON.stringify(config).includes(missing));
  assert.equal((await server.request('{broken')).error.code, -32700);
  assert.equal((await server.request(rpc(4, 'not/a/method'))).error.code, -32601);
  assert.equal((await server.request(rpc(5, 'tools/call', { name: 'rf_feedback_submit' }))).error.code, -32602);
});

test('real Streamable HTTP: JSON responses, notifications, diagnostics, origin/host/method boundaries', { timeout: 60_000 }, async (t) => {
  const server = await start(t, ['--transport', 'http', '--port', '0']);
  const startup = await server.line(server.stderr);
  const url = startup.match(/http:\/\/127\.0\.0\.1:\d+\/mcp/)?.[0];
  assert.ok(url, startup);
  const post = (body, extra = {}) => fetch(url, { method: 'POST', headers: { ...headers, ...extra },
    body: typeof body === 'string' ? body : JSON.stringify(body), signal: AbortSignal.timeout(55_000) });
  const init = await post(rpc(1, 'initialize', initialize));
  assert.equal(init.status, 200);
  assert.ok(init.headers.get('content-type').includes('application/json'));
  assert.equal(init.headers.get('mcp-session-id'), null);
  checkInitialize(await init.json());
  const notification = await post({ jsonrpc: '2.0', method: 'notifications/initialized' });
  assert.equal(notification.status, 202);
  assert.equal(await notification.text(), '');
  checkTools(await (await post(rpc(2, 'tools/list'))).json());
  checkDoctor(await (await post(rpc(3, 'tools/call', { name: 'rf_doctor', arguments: { repo_root: '.' } }))).json());
  assert.equal((await fetch(url)).status, 405);
  assert.equal((await post(rpc(4, 'ping'), { Origin: 'https://untrusted.example' })).status, 403);
  // fetch normalizes Host; use the raw HTTP client to actually send a hostile Host.
  const hostileHostStatus = await new Promise((resolve, reject) => {
    const request = http.get(url, { headers: { Host: 'untrusted.example' } }, (response) => {
      response.resume(); resolve(response.statusCode);
    });
    request.setTimeout(2000, () => request.destroy(new Error('HTTP test timeout')));
    request.on('error', reject);
  });
  assert.equal(hostileHostStatus, 403);
  assert.equal((await post(rpc(4, 'ping'), { 'Content-Type': 'text/plain' })).status, 415);
  assert.equal((await post(rpc(4, 'ping'), { Accept: 'text/html' })).status, 406);
  assert.equal((await post(rpc(4, 'ping'), { 'MCP-Protocol-Version': 'unknown' })).status, 400);
  assert.equal((await (await post('{broken')).json()).error.code, -32700);
  assert.equal((await post('x'.repeat(1024 * 1024 + 1))).status, 413);
});

test('tool argv is closed, positional paths are absolute, shell metacharacters remain a single value', () => {
  const hostile = 'RULE;$(touch SHOULD_NOT_EXIST)';
  assert.deepEqual(buildArgs('rf_scan', { repo_root: '.', rule_id: hostile }),
    ['scan', '--repo-root', process.cwd(), '--rule-id', hostile, '--format', 'json', '--progress', 'none']);
  assert.deepEqual(buildArgs('rf_scan', { repo_root: '.', candidate: 'existing' }),
    ['scan', '--repo-root', process.cwd(), '--candidate', path.resolve('existing'), '--format', 'json', '--progress', 'none']);
  assert.deepEqual(buildArgs('rf_explain', { repo_root: '.', finding: 'view.json', rule_meta: 'meta.json' }),
    ['explain', path.resolve('view.json'), '--rule-meta', path.resolve('meta.json'), '--format', 'json']);
  for (const args of [{}, { rule_id: 'x', candidate: 'y' }, { rule_id: '--host' }, { rule_id: 'a\n' },
    { rule_id: 'x', output: 'file' }, { rule_id: 'x', host: 'sh' }, { rule_id: 'x', format: 'sarif' }]) {
    assert.throws(() => buildArgs('rf_scan', args), { code: -32602 });
  }
  for (const name of DISABLED) assert.throws(() => buildArgs(name, {}), { code: -32602 });
  assert.throws(() => buildArgs('rf_doctor', []), { code: -32602 });
  assert.throws(() => buildArgs('rf_explain', {}), { code: -32602 });
});

test('output privacy fails closed without mutating CLI decisions', () => {
  const report = { kind: 'ScanResult', gateDecision: 'INCONCLUSIVE', authority: 'LOCAL_UNATTESTED',
    findingViews: [{ kind: 'FindingView', message: 'metadata only' }], resultDigest: 'sha256:original' };
  assert.equal(validateOutput('rf_scan', report), report);
  assert.equal(report.gateDecision, 'INCONCLUSIVE');
  assert.equal(report.resultDigest, 'sha256:original');
  for (const key of ['source', 'sourceCode', 'source_content', 'sourceSnippet', 'fullFinding', 'findings', 'rawPayload', 'unrecognizedExtra']) {
    assert.throws(() => validateOutput('rf_scan', { ...report, findingViews: [{ [key]: 'private' }] }));
  }
  assert.throws(() => validateOutput('rf_doctor', { kind: 'Unexpected' }));
  assert.throws(() => validateOutput('rf_scan', []));
});

test('JSON-RPC rejects batches, invalid ids and arguments; notification tools cannot execute', async () => {
  for (const value of [[], null, {}, rpc(null, 'ping'), rpc({}, 'ping'), rpc(2, 'tools/list', [])]) {
    assert.equal((await dispatch(value)).error.code, -32600);
  }
  assert.equal(await dispatch({ jsonrpc: '2.0', method: 'tools/call', params: { name: 'rf_scan' } }), null);
  assert.equal((await dispatch(rpc(1, 'initialize', {}))).error.code, -32602);
  assert.deepEqual(await dispatch(rpc('ping-id', 'ping')), { jsonrpc: '2.0', id: 'ping-id', result: {} });
  assert.equal((await dispatch(rpc(1, 'initialize', { ...initialize, protocolVersion: 'unknown' }))).result.protocolVersion, '2025-06-18');
});

test('HTTP defaults to loopback and rejects accidental or malformed listener configuration', () => {
  assert.deepEqual(options([]), { transport: 'stdio', host: '127.0.0.1', port: 3000 });
  assert.deepEqual(options(['--transport', 'http']), { transport: 'http', host: '127.0.0.1', port: 3000 });
  assert.equal(options(['--transport', 'http', '--host', '0.0.0.0']).host, '0.0.0.0');
  for (const args of [['--port', '2'], ['--transport', 'http', '--port', '-1'],
    ['--transport', 'http', '--port', '65536'], ['--transport', 'http', '--host', 'example.com'],
    ['--transport', 'other'], ['--unknown', 'x']]) assert.throws(() => options(args));
  assert.deepEqual(tools.map((entry) => entry.name), NAMES);
});
