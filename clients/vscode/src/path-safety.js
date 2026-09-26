'use strict';

const fs = require('fs');
const path = require('path');

// Resolve an existing workspace file without following a symlink in the
// workspace-relative path. FindingView paths are untrusted input; the lexical
// check in core.js is necessary but cannot detect a symlinked directory.
function resolveSafeWorkspaceFile(root, relativePath) {
  const rootAbsolute = path.resolve(root);
  const target = path.resolve(rootAbsolute, relativePath);
  if (target !== rootAbsolute && !target.startsWith(rootAbsolute + path.sep)) {
    throw new Error('TARGET_ESCAPES_WORKSPACE');
  }

  let cursor = rootAbsolute;
  const relative = path.relative(rootAbsolute, target);
  for (const segment of relative.split(path.sep)) {
    if (!segment) continue;
    cursor = path.join(cursor, segment);
    if (fs.lstatSync(cursor).isSymbolicLink()) {
      throw new Error('SYMLINK_COMPONENT');
    }
  }

  const rootReal = fs.realpathSync.native(rootAbsolute);
  const targetReal = fs.realpathSync.native(target);
  if (targetReal !== rootReal && !targetReal.startsWith(rootReal + path.sep)) {
    throw new Error('REAL_TARGET_ESCAPES_WORKSPACE');
  }
  if (!fs.statSync(target).isFile()) {
    throw new Error('TARGET_NOT_REGULAR_FILE');
  }
  return target;
}

module.exports = { resolveSafeWorkspaceFile };
