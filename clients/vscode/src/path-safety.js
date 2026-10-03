'use strict';

const fs = require('fs');
const path = require('path');

// Resolve an existing workspace file without following a symlink in the
// workspace-relative path. FindingView paths are untrusted input; the lexical
// check in core.js is necessary but cannot detect a symlinked directory.
function resolveSafeWorkspacePath(root, relativePath, directory) {
  const rootAbsolute = path.resolve(root);
  const target = path.resolve(rootAbsolute, relativePath);
  const relative = path.relative(rootAbsolute, target);
  if (relative === '..' || relative.startsWith('..' + path.sep) || path.isAbsolute(relative)) {
    throw new Error('TARGET_ESCAPES_WORKSPACE');
  }

  let cursor = rootAbsolute;
  for (const segment of relative.split(path.sep)) {
    if (!segment) continue;
    cursor = path.join(cursor, segment);
    if (fs.lstatSync(cursor).isSymbolicLink()) {
      throw new Error('SYMLINK_COMPONENT');
    }
  }

  const rootReal = fs.realpathSync.native(rootAbsolute);
  const targetReal = fs.realpathSync.native(target);
  const realRelative = path.relative(rootReal, targetReal);
  if (realRelative === '..' || realRelative.startsWith('..' + path.sep) || path.isAbsolute(realRelative)) {
    throw new Error('REAL_TARGET_ESCAPES_WORKSPACE');
  }
  const stat = fs.statSync(target);
  if (directory ? !stat.isDirectory() : !stat.isFile()) {
    throw new Error(directory ? 'TARGET_NOT_DIRECTORY' : 'TARGET_NOT_REGULAR_FILE');
  }
  return target;
}

function resolveSafeWorkspaceFile(root, relativePath) {
  return resolveSafeWorkspacePath(root, relativePath, false);
}
function resolveSafeWorkspaceDirectory(root, relativePath) {
  return resolveSafeWorkspacePath(root, relativePath, true);
}
// Only native file-picker outputs / their persisted artifact records may use
// this gate. Finding navigation and scopes must still use the workspace gate.
function resolveSafeSelectedPath(selectedPath, directory = false) {
  if (!path.isAbsolute(selectedPath)) throw new Error('SELECTED_PATH_NOT_ABSOLUTE');
  const filesystemRoot = path.parse(selectedPath).root;
  return resolveSafeWorkspacePath(filesystemRoot, path.relative(filesystemRoot, selectedPath), directory);
}
module.exports = { resolveSafeWorkspaceFile, resolveSafeWorkspaceDirectory, resolveSafeSelectedPath };
