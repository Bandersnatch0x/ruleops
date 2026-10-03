'use strict';
const { spawn } = require('node:child_process');
const MAX_OUTPUT_BYTES = 1_048_576;

// argv only; cancellation, timeout and output overflow terminate the process tree.
function runRf(args, config, cwd, signal) {
  return new Promise((resolve) => {
    if (signal?.aborted) return resolve({ code: null, stdout: '', stderr: '', reason: 'CANCELLED' });
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
    const timer = setTimeout(() => stop('TIMEOUT'), config.requestTimeoutMs);
    signal?.addEventListener('abort', cancel, { once: true });
    for (const name of ['stdout', 'stderr']) child[name].on('data', (chunk) => {
      if (reason) return;
      sizes[name] += chunk.length;
      if (sizes[name] > MAX_OUTPUT_BYTES) return stop('OUTPUT_LIMIT');
      chunks[name].push(chunk);
    });
    child.on('error', (err) => { reason = err.code === 'ENOENT' ? 'UNAVAILABLE' : 'PROCESS_ERROR'; error = err.message; finish(-1); });
    child.on('close', finish);
  });
}
module.exports = { runRf };
