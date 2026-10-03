// Opt-in adapter integration: real rf CLI and disk, explicit loopback contract server.
// The VS Code host is a test double; this is not installed-IDE or production-server evidence.
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const { host, findingFile } = require('./sidebar-host');
const { runRf } = require('../src/rf-invoker');

test('real CLI saves then submits the same request and queries both selectors', { skip: !process.env.ERF_REAL_RF, timeout: 60000 }, async t => {
  // The development launcher resolves Cargo from cwd; stay beneath this checkout.
  const parent = path.resolve(__dirname, '../../../.scratch'); fs.mkdirSync(parent, { recursive: true });
  const file = findingFile(t, {}, parent); const root = path.dirname(file); const rfPath = process.env.ERF_REAL_RF;
  fs.writeFileSync(path.join(root, 'pom.xml'), '<project><modelVersion>4.0.0</modelVersion><groupId>test</groupId><artifactId>ide-stage2</artifactId><version>1</version></project>');
  const config = { rfPath, requestTimeoutMs: 15000 };
  const init = await runRf(['init', '--repo-root', root, '--non-interactive'], config, root);
  assert.equal(init.code, 0, init.stderr);
  const received = []; const queries = []; let artifact;
  const server = http.createServer((req, res) => {
    let body = ''; req.setEncoding('utf8'); req.on('data', chunk => { body += chunk; });
    req.on('end', () => {
      let response;
      if (req.method === 'POST') {
        const envelope = JSON.parse(body); artifact = envelope.payload;
        received.push({ path: req.url, envelope, key: req.headers['idempotency-key'] });
        response = { apiVersion: 'rulefactory.io/v1', requestId: 'test-server-1', replayed: false,
          resource: { type: req.url.slice('/v1/'.length), id: envelope.id, version: envelope.version, state: 'OPEN' } };
      } else {
        queries.push(req.url);
        const resourceType = artifact.kind === 'FeedbackDraft' ? 'feedback' : 'exceptions';
        response = { apiVersion: 'rulefactory.io/v1', kind: 'SubmissionStatus', schemaVersion: 'submission-status-v1', status: 'RECEIVED_ONLY', requestId: artifact.requestId,
          artifactDigest: artifact.digest, serverRequestId: 'test-status-1', replayed: false, automaticRetry: false, authority: 'RECEIVED_ONLY',
          resource: { type: resourceType, id: artifact.requestId, version: artifact.schemaVersion, state: 'OPEN', digest: 'sha256:' + 'a'.repeat(64) },
          auditContext: { operation: 'CREATE', operationId: artifact.requestId, actor: artifact.actor, reason: artifact.reason, resourceType, resourceId: artifact.requestId,
            resourceVersion: artifact.schemaVersion, eventDigest: 'sha256:' + 'b'.repeat(64), recordedAt: '2026-10-03T00:00:00Z' } };
      }
      const bytes = JSON.stringify(response); res.writeHead(200, { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(bytes), Connection: 'close' }); res.end(bytes);
    });
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  config.serverEndpoint = `http://127.0.0.1:${server.address().port}/v1`;
  const calls = [];
  const h = host(async (...args) => { calls.push(args[0]); return runRf(...args); }, { file, root, config, realConfig: true });
  for (const kind of ['exception', 'feedback']) {
    await h.sidebar.open(kind === 'exception' ? 'erf.requestException' : 'erf.draftFeedback');
    await h.sidebar.message({ type: 'pickFile' });
    assert.equal(h.sidebar.state.phase, 'ready', h.sidebar.state.detail);
    // No scope/requestId/artifactDigest is typed; selected object supplies all three.
    await h.sidebar.execute({ actor: 'integration-tester', reason: 'explicit adapter integration test' });
    assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
    const record = h.sidebar.requests[0]; const savedBytes = fs.readFileSync(record.path, 'utf8');
    assert.ok(h.sidebar.state.detail.includes(record.path));
    const saveArgs = calls.findLast(args => args[0] === kind && ['request', 'draft'].includes(args[1]));
    assert.equal(saveArgs[saveArgs.indexOf('--output') + 1], record.path);
    await h.sidebar.open(kind === 'exception' ? 'erf.submitException' : 'erf.submitFeedback'); await h.sidebar.execute({});
    assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
    const submitArgs = calls.findLast(args => args[1] === 'submit'); assert.equal(submitArgs[2], record.path);
    assert.equal(fs.readFileSync(record.path, 'utf8'), savedBytes);
    assert.deepEqual(received.at(-1).envelope.payload, JSON.parse(savedBytes)); assert.equal(received.at(-1).key, record.requestId);
    assert.equal(h.sidebar.requests[0].receipt.status, 'SUBMITTED');
    await h.sidebar.open('erf.submissionStatus'); await h.sidebar.execute({});
    assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
    await h.sidebar.execute({ selector: 'artifactDigest' }); assert.equal(h.sidebar.state.phase, 'completed', h.sidebar.state.detail);
    assert.ok(queries.at(-2).includes('requestId=')); assert.ok(queries.at(-1).includes('artifactDigest='));
    console.log(JSON.stringify({ kind, savedPath: record.path, submitPath: submitArgs[2], requestId: record.requestId, artifactDigest: record.artifactDigest, savedFileUnchanged: true, receivedPayloadMatches: true, selectors: ['requestId', 'artifactDigest'], authority: 'RECEIVED_ONLY' }));
  }
  // The existing CLI rejects control characters: do not rewrite the user's reason.
  await h.sidebar.open('erf.draftFeedback'); await h.sidebar.execute({ actor: 'integration-tester', reason: 'line one\nline two' });
  assert.equal(h.sidebar.state.phase, 'failed'); assert.doesNotMatch(h.sidebar.state.detail, /已保存/);
});
