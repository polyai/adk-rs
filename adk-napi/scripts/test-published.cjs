#!/usr/bin/env node

const { execFileSync } = require("node:child_process");
const { mkdtempSync, rmSync, writeFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const path = require("node:path");

const packageSpec = process.argv[2] || process.env.ADK_NAPI_PUBLISHED_PACKAGE || "@poly-ai/adk-node@rc";
const packageName = "@poly-ai/adk-node";
const smokeDir = mkdtempSync(path.join(tmpdir(), "adk-napi-published-"));
const helperPath = path.resolve(__dirname, "../test/wrapper_cases.js");
const testPath = path.join(smokeDir, "published-wrapper.test.cjs");

try {
  writeFileSync(
    path.join(smokeDir, "package.json"),
    `${JSON.stringify({ private: true, type: "commonjs" }, null, 2)}\n`,
  );

  execFileSync("npm", ["install", "--no-audit", "--no-fund", packageSpec], {
    cwd: smokeDir,
    stdio: "inherit",
  });

  writeFileSync(
    testPath,
    `
const wrapper = require(${JSON.stringify(packageName)});
const { runWrapperTests } = require(${JSON.stringify(helperPath)});

runWrapperTests(wrapper);
`,
  );

  execFileSync(process.execPath, ["--test", testPath], {
    cwd: smokeDir,
    stdio: "inherit",
  });

  console.log(`Published package smoke test passed for ${packageSpec}`);
} finally {
  if (process.env.ADK_NAPI_KEEP_PUBLISHED_SMOKE_DIR) {
    console.log(`Preserving published package smoke test directory: ${smokeDir}`);
  } else {
    rmSync(smokeDir, { recursive: true, force: true });
  }
}
