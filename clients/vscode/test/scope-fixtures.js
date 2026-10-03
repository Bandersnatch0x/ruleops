const fs = require('node:fs');
const path = require('node:path');
const core = require('../src/core');
const fixture = path.resolve(__dirname, '../../testdata/scope-options.tsv');
const list = value => value ? value.split(';') : [];
const cases = fs.readFileSync(fixture, 'utf8').trimEnd().split('\n').slice(1).map(line => {
  const [name, findingPath, modules, roots, files, directories, expected = ''] = line.split('\t');
  const finding = { apiVersion: 'rulefactory.io/v1', kind: 'FindingView', findingId: 'f', ruleId: 'R1', fingerprint: 'fp', message: 'safe', location: { path: findingPath, line: 1, column: 1 } };
  return { name, finding, config: { modules: list(modules), source_roots: list(roots) }, facts: { files: list(files), directories: list(directories) }, expected: expected === '-' ? '' : expected };
});
const projection = c => core.projectScopeOptions(c.finding, c.config, c.facts).map(o => `${o.value}|${o.source}|${o.broader}`).join(';');
module.exports = { cases, projection };
