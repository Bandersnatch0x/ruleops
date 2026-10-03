'use strict';
const fs = require('node:fs');
const path = require('node:path');
fs.copyFileSync(path.join(__dirname, '../../ide-command-map.json'), path.join(__dirname, '../resources/ide-command-map.json'));
