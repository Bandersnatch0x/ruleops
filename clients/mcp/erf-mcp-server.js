#!/usr/bin/env node
'use strict';

const { execFile } = require('node:child_process');
const { createHash } = require('node:crypto');
const http = require('node:http');
const { isIP } = require('node:net');
const path = require('node:path');

const PROTOCOL_VERSION = '2025-06-18';
const MAX_BYTES = 1024 * 1024;
const RF_TIMEOUT_MS = 50_000;
const RF = path.resolve(__dirname, '../../rf');
const text = (description) => ({ type: 'string', minLength: 1, description });
const root = { repo_root: text('Repository path, relative to the server working directory; default .') };
const tools = [
  {
    name: 'rf_doctor', description: 'Read rf environment/configuration diagnostics; never a Gate decision.',
    inputSchema: { type: 'object', properties: root, additionalProperties: false },
  },
  {
    name: 'rf_config_show', description: 'Read effective rf project configuration and identity digests.',
    inputSchema: { type: 'object', properties: root, additionalProperties: false },
  },
  {
    name: 'rf_scan', description: 'Run rf scan and return its digest-only projection without changing its decision.',
    inputSchema: {
      type: 'object', additionalProperties: false,
      properties: { ...root, rule_id: text('Rule identifier'), candidate: text('Existing Candidate directory') },
      oneOf: [{ required: ['rule_id'] }, { required: ['candidate'] }],
    },
  },
  {
    name: 'rf_explain', description: 'Explain an existing digest-only FindingView; no authority is added.',
    inputSchema: {
      type: 'object', additionalProperties: false, required: ['finding'],
      properties: { ...root, finding: text('FindingView JSON file'), rule_meta: text('Existing rule metadata JSON file') },
    },
  },
];

class RpcError extends Error {
  constructor(code, message) { super(message); this.code = code; }
}

function object(value) { return value !== null && typeof value === 'object' && !Array.isArray(value); }
function invalid(message) { throw new RpcError(-32602, message); }

function buildArgs(name, args) {
  const tool = tools.find((entry) => entry.name === name);
  if (!tool) invalid('Unknown or disabled tool');
  if (!object(args)) invalid('arguments must be an object');
  for (const [key, value] of Object.entries(args)) {
    if (!Object.hasOwn(tool.inputSchema.properties, key)) invalid('Unknown argument');
    if (typeof value !== 'string' || !value.trim() || value.startsWith('-') || /[\x00-\x1f\x7f]/.test(value)) {
      invalid('Arguments must be non-empty strings without control characters or leading options');
    }
  }
  for (const key of tool.inputSchema.required || []) {
    if (!Object.hasOwn(args, key)) invalid('Missing required argument: ' + key);
  }
  const repo = path.resolve(args.repo_root || '.');
  if (name === 'rf_doctor') return ['doctor', '--repo-root', repo, '--format', 'json'];
  if (name === 'rf_config_show') return ['config', 'show', '--repo-root', repo, '--format', 'json'];
  if (name === 'rf_scan') {
    if (Boolean(args.rule_id) === Boolean(args.candidate)) invalid('Provide exactly one of rule_id or candidate');
    return ['scan', '--repo-root', repo, ...(args.rule_id ? ['--rule-id', args.rule_id] :
      ['--candidate', path.resolve(repo, args.candidate)]), '--format', 'json', '--progress', 'none'];
  }
  return ['explain', path.resolve(repo, args.finding),
    ...(args.rule_meta ? ['--rule-meta', path.resolve(repo, args.rule_meta)] : []), '--format', 'json'];
}

// Closed output fields mirror the CLI projections, not its decision logic. Unknown fields fail closed.
const configFields = 'version language build_system modules source_roots test_roots generated_roots vendor_roots include exclude execution_mode cache_dir trust_state_path';
const outputFields = {
  DoctorReport: 'api_version kind repository mode overall identity sources checks',
  ConfigShow: 'config configPath identity sources',
  ScanResult: 'apiVersion kind schemaVersion status gateDecision authority trust ruleId planned executed failed completeness planDigest effectiveConfigDigest workspaceDescriptorDigest resolvedWorkspaceDescriptorDigest artifactPath bundleDigest bundleDeliveryDescriptorDigest bundleCompatibilityIdentity bundlePlatform configScanPlanDigest semanticResultDigest findingViews findingRecords diagnostics progress resultDigest',
  ScanFailure: 'apiVersion kind schemaVersion exitCode diagnostics progress diagnosticDigest',
  RichExplanation: 'apiVersion kind schemaVersion authoritative findingId ruleId fingerprint message location findingDigest authorityStatus rule assessment nextStep explanationDigest',
  RichExplanationRefusal: 'apiVersion kind schemaVersion authoritative refused reason findingId ruleId nextStep refusalDigest',
};
const nestedFields = {
  config: configFields, sources: configFields,
  identity: 'config_digest workspace_descriptor_digest scan_plan_digest',
  checks: 'code status detail nextStep',
  completeness: 'planned accounted failed missing observationErrors complete',
  findingViews: 'apiVersion kind findingId ruleId fingerprint message location',
  findingRecords: 'findingId ruleId fingerprint findingDigest message location',
  location: 'kind path line column identityDigest',
  diagnostics: 'code stage message retry nextStep detailDigest reasonCodes',
  progress: 'stage outcome elapsedNanos inputDigest',
  rule: 'ruleVersion severity status rationale remediation standardId standardVersion bindingDigest',
  assessment: 'baseline exception',
};
const outputKinds = {
  rf_doctor: ['DoctorReport'], rf_scan: ['ScanResult', 'ScanFailure'],
  rf_explain: ['RichExplanation', 'RichExplanationRefusal'],
};
function validateOutput(name, value) {
  if (!object(value)) throw new Error('RF_OUTPUT_SHAPE');
  const kind = name === 'rf_config_show' ? 'ConfigShow' : value.kind;
  if (name === 'rf_config_show') {
    if (!object(value.config) || !object(value.identity) || !object(value.sources)) throw new Error('RF_OUTPUT_SHAPE');
  } else if (!outputKinds[name]?.includes(kind)) throw new Error('RF_OUTPUT_SHAPE');
  function visit(item, fields, depth) {
    if (depth > 32) throw new Error('RF_OUTPUT_DEPTH');
    if (!item || typeof item !== 'object') return;
    if (Array.isArray(item)) { item.forEach((child) => visit(child, fields, depth + 1)); return; }
    if (!fields) throw new Error('RF_OUTPUT_PRIVACY');
    for (const [key, child] of Object.entries(item)) {
      if (!fields.split(' ').includes(key)) throw new Error('RF_OUTPUT_PRIVACY');
      visit(child, Object.hasOwn(nestedFields, key) ? nestedFields[key] : undefined, depth + 1);
    }
  }
  visit(value, outputFields[kind], 0);
  return value;
}

function digest(value) { return 'sha256:' + createHash('sha256').update(value).digest('hex'); }
function toolResult(value, isError, exitCode) {
  return { content: [{ type: 'text', text: JSON.stringify(value) }], structuredContent: value,
    isError, _meta: { 'erf/exitCode': exitCode } };
}

async function callTool(name, args) {
  const argv = buildArgs(name, args);
  // Executable is pinned to this installation, not supplied by an MCP request. Never invoke a shell.
  return new Promise((resolve) => {
    execFile(RF, argv, { shell: false, encoding: 'utf8', timeout: RF_TIMEOUT_MS,
      killSignal: 'SIGKILL', maxBuffer: MAX_BYTES }, (error, stdout, stderr) => {
      const exitCode = error ? (Number.isInteger(error.code) ? error.code : null) : 0;
      const failure = (code) => toolResult({ error: code, exitCode,
        stdoutDigest: digest(stdout), stderrDigest: digest(stderr) }, true, exitCode);
      if (error && !Number.isInteger(error.code)) {
        resolve(failure(error.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER' ? 'RF_OUTPUT_TOO_LARGE' :
          error.killed ? 'RF_TIMEOUT' : 'RF_EXECUTION_ERROR'));
        return;
      }
      let value;
      try { value = JSON.parse(stdout); } catch { resolve(failure('RF_INVALID_JSON')); return; }
      try { validateOutput(name, value); } catch { resolve(failure('RF_OUTPUT_REJECTED')); return; }
      resolve(toolResult(value, exitCode !== 0, exitCode));
    });
  });
}

async function dispatch(request) {
  const validId = object(request) && (typeof request.id === 'string' ||
    (typeof request.id === 'number' && Number.isFinite(request.id)));
  if (!object(request) || request.jsonrpc !== '2.0' || typeof request.method !== 'string' ||
    (Object.hasOwn(request, 'id') && !validId) ||
    (Object.hasOwn(request, 'params') && !object(request.params))) {
    return { jsonrpc: '2.0', id: validId ? request.id : null, error: { code: -32600, message: 'Invalid Request' } };
  }
  // Notifications never trigger tools or produce JSON-RPC responses.
  if (!Object.hasOwn(request, 'id')) return null;
  try {
    const params = request.params || {};
    let result;
    switch (request.method) {
      case 'initialize':
        if (typeof params.protocolVersion !== 'string' || !object(params.capabilities) ||
          !object(params.clientInfo) || typeof params.clientInfo.name !== 'string' ||
          typeof params.clientInfo.version !== 'string') invalid('Invalid initialize parameters');
        result = { protocolVersion: PROTOCOL_VERSION, capabilities: { tools: { listChanged: false } },
          serverInfo: { name: 'erf-mcp-server', version: '0.1.0' },
          instructions: 'Thin rf adapter. Tool errors and diagnostics are not Gate PASS, approval or SCM authority.' };
        break;
      case 'ping': result = {}; break;
      case 'tools/list': result = { tools }; break;
      case 'tools/call':
        if (typeof params.name !== 'string') invalid('Missing tool name');
        result = await callTool(params.name, params.arguments === undefined ? {} : params.arguments);
        break;
      default: throw new RpcError(-32601, 'Method not found');
    }
    return { jsonrpc: '2.0', id: request.id, result };
  } catch (error) {
    if (!(error instanceof RpcError)) throw error;
    return { jsonrpc: '2.0', id: request.id, error: { code: error.code, message: error.message } };
  }
}

async function message(raw) {
  let request;
  try { request = JSON.parse(raw); } catch {
    return { jsonrpc: '2.0', id: null, error: { code: -32700, message: 'Parse error' } };
  }
  return dispatch(request);
}

async function stdio() {
  process.stdin.setEncoding('utf8');
  let pending = '';
  for await (const chunk of process.stdin) {
    pending += chunk;
    let newline;
    while ((newline = pending.indexOf('\n')) !== -1) {
      const line = pending.slice(0, newline);
      pending = pending.slice(newline + 1);
      if (Buffer.byteLength(line) > MAX_BYTES) throw new Error('MCP_REQUEST_TOO_LARGE');
      const response = await message(line);
      if (response) process.stdout.write(JSON.stringify(response) + '\n');
    }
    if (Buffer.byteLength(pending) > MAX_BYTES) throw new Error('MCP_REQUEST_TOO_LARGE');
  }
  if (pending.trim()) throw new Error('MCP_UNTERMINATED_MESSAGE');
}

function options(argv) {
  const result = { transport: 'stdio', host: '127.0.0.1', port: 3000 };
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i].slice(2);
    if (!['--transport', '--host', '--port'].includes(argv[i]) || argv[i + 1] === undefined) {
      throw new Error('Usage: erf-mcp-server.js [--transport stdio|http] [--host IP] [--port 0..65535]');
    }
    result[key] = argv[i + 1];
  }
  if (!['stdio', 'http'].includes(result.transport) || !isIP(result.host) ||
    !/^\d+$/.test(String(result.port)) || Number(result.port) > 65535) throw new Error('Invalid transport, IP or port');
  if (result.transport !== 'http' && argv.some((arg) => ['--host', '--port'].includes(arg))) {
    throw new Error('--host and --port require --transport http');
  }
  result.port = Number(result.port);
  return result;
}

function serveHttp(config) {
  const loopback = config.host === '127.0.0.1' || config.host === '::1';
  if (!loopback) console.error('WARNING: non-loopback MCP listener; no authentication. Use a trusted network and authenticated TLS proxy.');
  const server = http.createServer(async (request, response) => {
    const reply = (status, value) => {
      response.writeHead(status, { ...(value === undefined ? {} : { 'Content-Type': 'application/json' }),
        'Cache-Control': 'no-store', ...(status === 405 ? { Allow: 'POST' } : {}) });
      response.end(value === undefined ? undefined : JSON.stringify(value));
    };
    const port = server.address().port;
    const allowedHosts = new Set([`127.0.0.1:${port}`, `localhost:${port}`, `[::1]:${port}`]);
    if (loopback && !allowedHosts.has(request.headers.host)) { reply(403); return; }
    if (request.headers.origin && request.headers.origin !== `http://${request.headers.host}`) { reply(403); return; }
    if (request.url !== '/mcp') { reply(404); return; }
    if (request.method !== 'POST') { reply(405); return; }
    if (request.headers['content-type']?.split(';')[0].trim().toLowerCase() !== 'application/json') { reply(415); return; }
    const accept = request.headers.accept || '';
    if (!accept.includes('application/json') || !accept.includes('text/event-stream')) { reply(406); return; }
    const version = request.headers['mcp-protocol-version'];
    if (version !== undefined && version !== PROTOCOL_VERSION) { reply(400); return; }
    let size = 0;
    const chunks = [];
    try {
      for await (const chunk of request) {
        size += chunk.length;
        if (size <= MAX_BYTES) chunks.push(chunk);
      }
      if (size > MAX_BYTES) { reply(413); return; }
      const result = await message(Buffer.concat(chunks).toString('utf8'));
      if (result === null) { reply(202); return; }
      reply(200, result);
    } catch (error) {
      console.error('MCP_HTTP_ERROR:', error.code || error.name);
      if (!response.headersSent) reply(500, { jsonrpc: '2.0', id: null,
        error: { code: -32603, message: 'Internal error' } });
    }
  });
  server.requestTimeout = 60_000;
  server.headersTimeout = 10_000;
  server.on('error', (error) => { console.error('MCP_LISTEN_ERROR:', error.code); process.exitCode = 1; });
  server.listen(config.port, config.host, () => {
    const host = config.host.includes(':') ? `[${config.host}]` : config.host;
    console.error(`ERF MCP listening http://${host}:${server.address().port}/mcp`);
  });
  return server;
}

async function main() {
  const config = options(process.argv.slice(2));
  if (config.transport === 'http') serveHttp(config);
  else await stdio();
}

if (require.main === module) main().catch((error) => { console.error('MCP_SERVER_ERROR:', error.message); process.exitCode = 1; });
module.exports = { buildArgs, validateOutput, dispatch, options, tools };
