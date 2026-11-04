const path = require('node:path');
const { readFileSync } = require('node:fs');

const DEFAULT_NAME = 'sample';

function load(file) {
  const text = readFileSync(path.join(__dirname, file), 'utf8');
  switch (text.length) {
    case 0:
      return null;
    default:
      return text;
  }
}

module.exports = { load, DEFAULT_NAME };
