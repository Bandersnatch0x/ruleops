'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const { tools } = require('../server');
const source = readFileSync(`${__dirname}/../app.js`, 'utf8');

function load(fetchTools, supported = true) {
  const nodes = new Map();
  function element() {
    return { value: '', children: [], textContent: '', disabled: true,
      addEventListener() {},
      replaceChildren() { this.children = []; this.value = ''; },
      append(child) { this.children.push(child); if (!this.value) this.value = child.value || ''; },
    };
  }
  const definitions = [];
  const document = {
    getElementById(id) { if (!nodes.has(id)) nodes.set(id, element()); return nodes.get(id); },
    createElement: element,
    modelContext: supported ? { registerTool(tool) { definitions.push(tool); } } : undefined,
  };
  vm.runInNewContext(source, { document, AbortSignal, fetch: (...args) => {
    // Registration and the manual controls must already be ready before any network request.
    assert.equal(definitions.length, supported ? 4 : 0);
    assert.equal(nodes.get('run').disabled, false);
    return fetchTools(...args);
  } });
  return { nodes, definitions };
}
const enabled = 'WebMCP: 已启用（Chrome 146+）';

test('offline schemas match the server and registration never waits for fetch', () => {
  const { nodes, definitions } = load(() => new Promise(() => {}));
  assert.equal(nodes.get('webmcp').textContent, enabled);
  assert.deepEqual(JSON.parse(JSON.stringify(definitions)), tools);
  assert.ok(definitions.every((tool) => typeof tool.execute === 'function'));
  assert.equal(nodes.get('tool').children.length, 4);
});

test('network failure, timeout and HTTP failure leave registration and manual controls usable', async () => {
  for (const response of [
    () => Promise.reject(new Error('offline')),
    () => Promise.reject(new DOMException('timed out', 'TimeoutError')),
    () => Promise.resolve({ ok: false, status: 503 }),
  ]) {
    const { nodes, definitions } = load(response);
    await new Promise(setImmediate);
    assert.equal(nodes.get('webmcp').textContent, enabled);
    assert.equal(definitions.length, 4);
    assert.equal(nodes.get('tool').children.length, 4);
    assert.equal(nodes.get('run').disabled, false);
    assert.match(nodes.get('status').textContent, /刷新失败/);
  }
});

test('successful fetch only refreshes manual tool choices, not registrations', async () => {
  const { nodes, definitions } = load((url, options) => {
    assert.equal(url, '/api/tools');
    assert.ok(options.signal instanceof AbortSignal);
    return Promise.resolve({ ok: true, json: async () => ({ tools: [tools[1]] }) });
  });
  await new Promise(setImmediate);
  assert.equal(definitions.length, 4);
  assert.equal(nodes.get('webmcp').textContent, enabled);
  assert.deepEqual(nodes.get('tool').children.map((option) => option.value), ['config_show']);
});

test('unsupported browsers remain usable without network', () => {
  const { nodes } = load(() => new Promise(() => {}), false);
  assert.equal(nodes.get('webmcp').textContent, 'WebMCP: 未启用（此浏览器不支持，仍可手动查看）');
  assert.equal(nodes.get('run').disabled, false);
});
