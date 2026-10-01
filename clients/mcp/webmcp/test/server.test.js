'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const childProcess = require('node:child_process');
const path = require('node:path');
let invocation;
const original = childProcess.execFile;
// Capture the actual shared adapter's process boundary, not a duplicate argv builder.
childProcess.execFile = (file, argv, options, callback) => {
  invocation = { file, argv, options };
  callback({ code: 2 }, JSON.stringify({ kind: 'DoctorReport', checks: [{ code: 'CONFIG_MISSING', status: 'FAIL' }] }), '');
};
const { createServer, HOST } = require('../server');
childProcess.execFile = original;

test('loopback HTTP boundary, shared whitelist and argv execution', { timeout: 10_000 }, async (t) => {
  assert.equal(HOST, '127.0.0.1');
  const server = createServer();
  await new Promise((resolve) => server.listen(0, HOST, resolve));
  t.after(() => new Promise((resolve) => { server.close(resolve); server.closeAllConnections(); }));
  assert.equal(server.address().address, '127.0.0.1');
  const url = `http://${HOST}:${server.address().port}`;
  const post = (body, headers = {}) => fetch(`${url}/api/rf`, { method: 'POST',
    headers: { 'Content-Type': 'application/json', ...headers }, body: JSON.stringify(body) });
  const response = await fetch(`${url}/api/tools`);
  assert.equal(response.status, 200);
  const { tools } = await response.json();
  assert.deepEqual(tools.map((tool) => tool.name), ['doctor', 'config_show', 'scan', 'explain']);
  assert.ok(tools.every((tool) => tool.annotations.readOnlyHint && tool.inputSchema.additionalProperties === false));
  for (const body of [
    { tool: 'publish' }, { tool: 'rf_doctor' }, { tool: 'doctor', args: { output: '/tmp/out' } },
    { tool: 'doctor', args: { repo_root: '--help' } }, { tool: 'doctor', args: [] },
    { tool: 'scan', args: {} }, { tool: 'explain', args: {} }, { tool: 'doctor', extra: true },
  ]) assert.equal((await post(body)).status, 400);
  assert.equal(invocation, undefined);
  assert.equal((await post({ tool: 'doctor' }, { Origin: 'https://evil.example' })).status, 403);
  const hostStatus = await new Promise((resolve, reject) => { require('node:http').get(`${url}/api/tools`, { headers: { Host: 'evil.example' } }, (res) => { res.resume(); resolve(res.statusCode); }).on('error', reject); });
  assert.equal(hostStatus, 403);
  const payload = 'repo; touch PWNED $(id)';
  const resultResponse = await post({ tool: 'doctor', args: { repo_root: payload } });
  const result = await resultResponse.json();
  assert.equal(resultResponse.status, 200);
  assert.equal(resultResponse.headers.get('access-control-allow-origin'), null);
  assert.equal(result.isError, true);
  assert.equal(result._meta['erf/exitCode'], 2);
  assert.equal(result.structuredContent.checks[0].code, 'CONFIG_MISSING');
  assert.equal(invocation.file, path.resolve(__dirname, '../../../../rf'));
  assert.deepEqual(invocation.argv, ['doctor', '--repo-root', path.resolve(payload), '--format', 'json']);
  assert.equal(invocation.options.shell, false);
  assert.ok(invocation.options.timeout <= 60_000);
  assert.equal((await fetch(url)).status, 200);
  assert.equal((await fetch(`${url}/app.js`)).status, 200);
});
