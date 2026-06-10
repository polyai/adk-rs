'use strict';

const fs = require('node:fs');
const path = require('node:path');

const explicitBinary = process.env.POLY_ADK_NAPI_BINARY;

if (explicitBinary) {
  module.exports = require(explicitBinary);
  return;
}

const triples = platformTriples(process.platform, process.arch);
const candidates = [];

for (const triple of triples) {
  candidates.push(path.join(__dirname, `adk_napi.${triple}.node`));
}

candidates.push(path.join(__dirname, 'adk_napi.node'));

const loadErrors = [];

for (const candidate of candidates) {
  if (!fs.existsSync(candidate)) {
    continue;
  }
  try {
    module.exports = require(candidate);
    return;
  } catch (error) {
    loadErrors.push(`${candidate}: ${error.message}`);
  }
}

const searched = candidates.map((candidate) => `  - ${candidate}`).join('\n');
const suffix = loadErrors.length === 0 ? '' : `\nLoad errors:\n${loadErrors.join('\n')}`;

throw new Error(`Unable to load poly-adk-napi native binding. Searched:\n${searched}${suffix}`);

function platformTriples(platform, arch) {
  switch (`${platform}:${arch}`) {
    case 'darwin:arm64':
      return ['darwin-arm64'];
    case 'darwin:x64':
      return ['darwin-x64'];
    case 'linux:arm64':
      return ['linux-arm64-gnu', 'linux-arm64-musl'];
    case 'linux:x64':
      return ['linux-x64-gnu', 'linux-x64-musl'];
    case 'win32:x64':
      return ['win32-x64-msvc'];
    case 'win32:arm64':
      return ['win32-arm64-msvc'];
    default:
      return [];
  }
}
