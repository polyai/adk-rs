# Published Package Smoke Test

This directory is a tiny downstream npm package used to smoke test the
published `@poly-ai/adk-node` package. It imports the wrapper through normal
package resolution, then reuses the shared wrapper test cases from `../test`.

Local runs use the `rc` dist-tag declared in `package.json`. The publish
workflow overrides that dependency with the exact version it just published,
which verifies that consumers can install and import the package from npm.
