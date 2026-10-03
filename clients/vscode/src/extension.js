'use strict';
const vscode = require('vscode');
const { createSidebar, SCENARIOS } = require('./sidebar');
const commandMap = require('../resources/ide-command-map.json');

function activate(context) {
  const sidebar = createSidebar(vscode, context, commandMap);
  for (const scene of SCENARIOS) {
    context.subscriptions.push(vscode.commands.registerCommand(scene.id, object => sidebar.open(scene.id, object)));
  }
  return sidebar;
}
function deactivate() {}
module.exports = { activate, deactivate };
