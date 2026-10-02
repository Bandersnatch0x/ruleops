'use strict';
const http = require('node:http');
const { readFileSync } = require('node:fs');
const { tools: backendTools, dispatch } = require('../erf-mcp-server');

// Reuse the backend whitelist, validation, pinned execFile argv and digest-only output.
const tools = backendTools.map((tool) => ({ ...tool, name: tool.name.replace(/^rf_/, ''),
  annotations: { readOnlyHint: true } }));
const HOST = '127.0.0.1';
const MAX_BODY = 16 * 1024;
const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);

function createServer() {
  const assets = { '/': ['text/html; charset=utf-8', readFileSync(`${__dirname}/index.html`)],
    '/app.js': ['text/javascript; charset=utf-8', readFileSync(`${__dirname}/app.js`)] };
  const server = http.createServer(async (req, res) => {
    const reply = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store',
        'X-Content-Type-Options': 'nosniff' });
      res.end(JSON.stringify(value));
    };
    const port = server.address().port;
    if (![`${HOST}:${port}`, `localhost:${port}`].includes(req.headers.host) ||
        (req.headers.origin && req.headers.origin !== `http://${req.headers.host}`)) {
      reply(403, { error: 'LOCAL_ORIGIN_REQUIRED' }); return;
    }
    if (req.method === 'GET' && Object.hasOwn(assets, req.url)) {
      const [type, body] = assets[req.url];
      res.writeHead(200, { 'Content-Type': type, 'Cache-Control': 'no-store',
        'X-Content-Type-Options': 'nosniff',
        'Content-Security-Policy': "default-src 'self'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'" });
      res.end(body); return;
    }
    if (req.method === 'GET' && req.url === '/api/tools') { reply(200, { tools }); return; }
    if (req.url !== '/api/rf') { reply(404, { error: 'NOT_FOUND' }); return; }
    if (req.method !== 'POST') { reply(405, { error: 'POST_REQUIRED' }); return; }
    if (req.headers['content-type']?.split(';')[0].trim().toLowerCase() !== 'application/json') {
      reply(415, { error: 'JSON_REQUIRED' }); return;
    }
    try {
      let size = 0;
      const chunks = [];
      for await (const chunk of req) {
        size += chunk.length;
        if (size > MAX_BODY) { reply(413, { error: 'BODY_TOO_LARGE' }); return; }
        chunks.push(chunk);
      }
      let body;
      try { body = JSON.parse(Buffer.concat(chunks).toString('utf8')); }
      catch { reply(400, { error: 'INVALID_JSON' }); return; }
      if (!object(body) || Object.keys(body).some((key) => !['tool', 'args'].includes(key)) ||
          !tools.some((tool) => tool.name === body.tool)) {
        reply(400, { error: 'UNKNOWN_TOOL_OR_FIELD' }); return;
      }
      const result = await dispatch({ jsonrpc: '2.0', id: 1, method: 'tools/call',
        params: { name: `rf_${body.tool}`, arguments: body.args === undefined ? {} : body.args } });
      reply(result.error ? 400 : 200, result.error ? { error: result.error } : result.result);
    } catch (error) {
      console.error('WEBMCP_HTTP_ERROR', error);
      if (!res.headersSent) reply(500, { error: 'INTERNAL_ERROR' });
    }
  });
  server.requestTimeout = 60_000;
  server.headersTimeout = 10_000;
  return server;
}

if (require.main === module) {
  const port = process.env.PORT ?? '3210';
  if (!/^\d+$/.test(port) || Number(port) > 65535) throw new Error('PORT must be 0..65535');
  const server = createServer();
  server.on('error', (error) => { console.error('WEBMCP_LISTEN_ERROR', error); process.exitCode = 1; });
  server.listen(Number(port), HOST, () => console.log(`ERF WebMCP http://${HOST}:${server.address().port}`));
}
module.exports = { createServer, HOST, tools };
