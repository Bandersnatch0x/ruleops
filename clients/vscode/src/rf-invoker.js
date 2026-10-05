'use strict';
const { spawn } = require('node:child_process');
const { StringDecoder } = require('node:string_decoder');
const LOCAL_PROCESS_TIMEOUT_MS = 600_000;
const MAX_TIMER_MS = 2_147_483_647;
const MAX_OUTPUT_BYTES = 1_048_576;

// Local analysis can outlive an HTTP request; keep one policy for every caller.
function processTimeoutMs(args, config) {
  const local = ['scan', 'doctor'].includes(args[0])
    || (args[0] === 'config' && ['show', 'validate'].includes(args[1]))
    || (['standard', 'rule'].includes(args[0]) && args[1] === 'validate');
  const deadlineIndex = args[0] === 'scan' ? args.indexOf('--deadline-ms') : -1;
  const timeout = deadlineIndex >= 0 ? Number(args[deadlineIndex + 1])
    : local ? LOCAL_PROCESS_TIMEOUT_MS : config.requestTimeoutMs;
  if (!Number.isSafeInteger(timeout) || timeout <= 0 || timeout > MAX_TIMER_MS) throw new Error('INVALID_PROCESS_TIMEOUT');
  return timeout;
}

// argv only; cancellation, timeout and output overflow terminate the process tree.
function runRf(args, config, cwd, signal, { onStderrLine } = {}) {
  return new Promise((resolve) => {
    if (signal?.aborted) return resolve({ code: null, stdout: '', stderr: '', reason: 'CANCELLED' });
    const timeout = processTimeoutMs(args, config);
    const decoder = new StringDecoder('utf8');
    let pendingLine = '';
    const streamLines = (text, final = false) => {
      if (!onStderrLine) return;
      pendingLine += text;
      const lines = pendingLine.split('\n');
      pendingLine = lines.pop();
      if (final && pendingLine) { lines.push(pendingLine); pendingLine = ''; }
      for (const line of lines) onStderrLine(line.replace(/\r$/, ''));
    };
    const child = spawn(config.rfPath, args, { cwd, shell: false, windowsHide: true, detached: process.platform !== 'win32' });
    const chunks = { stdout: [], stderr: [] };
    const sizes = { stdout: 0, stderr: 0 };
    let reason;
    let error = '';
    let settled = false;
    const finish = (code) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancel);
      streamLines(decoder.end(), true);
      resolve({ code, stdout: Buffer.concat(chunks.stdout).toString('utf8'),
        stderr: error || Buffer.concat(chunks.stderr).toString('utf8'), reason });
    };
    const stop = (why) => {
      if (reason || settled) return;
      reason = why;
      if (!child.pid) return;
      if (process.platform === 'win32') {
        const killer = spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, shell: false });
        killer.on('error', (err) => { error = 'PROCESS_TREE_CANCEL_FAILED: ' + err.message; child.kill(); });
        killer.on('exit', (code) => { if (code !== 0 && !settled) { error = 'PROCESS_TREE_CANCEL_FAILED'; child.kill(); } });
      } else {
        try { process.kill(-child.pid, 'SIGKILL'); }
        catch (err) { if (err.code !== 'ESRCH') { error = 'PROCESS_TREE_CANCEL_FAILED: ' + err.message; child.kill(); } }
      }
    };
    const cancel = () => stop('CANCELLED');
    const timer = setTimeout(() => stop('TIMEOUT'), timeout);
    signal?.addEventListener('abort', cancel, { once: true });
    for (const name of ['stdout', 'stderr']) child[name].on('data', (chunk) => {
      if (reason) return;
      sizes[name] += chunk.length;
      if (sizes[name] > MAX_OUTPUT_BYTES) return stop('OUTPUT_LIMIT');
      chunks[name].push(chunk);
      if (name === 'stderr') streamLines(decoder.write(chunk));
    });
    child.on('error', (err) => { reason = err.code === 'ENOENT' ? 'UNAVAILABLE' : 'PROCESS_ERROR'; error = err.message; finish(-1); });
    child.on('close', finish);
  });
}
module.exports = { runRf, processTimeoutMs };
