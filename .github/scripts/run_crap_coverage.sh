#!/usr/bin/env bash
set -euo pipefail

workspace="$PWD"
output_dir="$PWD"
html_dir=""
baseline=""
pr_comment_output=""
repo_url=""
commit_ref=""
threshold="${CRAP_THRESHOLD:-30}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --workspace)
      workspace="$2"
      shift 2
      ;;
    --output-dir)
      output_dir="$2"
      shift 2
      ;;
    --html-dir)
      html_dir="$2"
      shift 2
      ;;
    --threshold)
      threshold="$2"
      shift 2
      ;;
    --baseline)
      baseline="$2"
      shift 2
      ;;
    --pr-comment-output)
      pr_comment_output="$2"
      shift 2
      ;;
    --repo-url)
      repo_url="$2"
      shift 2
      ;;
    --commit-ref)
      commit_ref="$2"
      shift 2
      ;;
    --no-html)
      html_dir=""
      shift
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

workspace="$(realpath "$workspace")"
mkdir -p "$output_dir"
output_dir="$(realpath "$output_dir")"

if [[ -n "$html_dir" ]]; then
  mkdir -p "$html_dir"
  html_dir="$(realpath "$html_dir")"
fi

cd "$workspace"

CRAP_EXCLUDES=(
  --exclude 'tests/**'
  --exclude 'src/tests.rs'
  --exclude 'src/testing.rs'
  --exclude 'src/*_test.rs'
  --exclude 'src/**/*_test.rs'
  --exclude 'src/*_tests.rs'
  --exclude 'src/**/*_tests.rs'
  --exclude 'src/**/tests.rs'
  --exclude 'src/**/testing.rs'
)
CRAP_ALLOWS=(
  # `adk-protobuf/src/*.rs` is prost-generated apart from the crate root.
  # Suppress generated enum mapper noise instead of chasing low-value tests.
  --allow 'adk-protobuf/src/*.rs'
)

cargo llvm-cov clean --workspace
cargo llvm-cov --workspace --no-report

(
  # Python imports the PyO3 extension as a subprocess, so it only contributes
  # raw .profraw files. Keep those files beside cargo-llvm-cov's Rust test
  # profiles so the final report command can merge both test runners.
  eval "$(cargo llvm-cov show-env --sh)"
  coverage_target_dir="$workspace/target/llvm-cov-target"
  export CARGO_TARGET_DIR="$coverage_target_dir"
  export LLVM_PROFILE_FILE="$coverage_target_dir/adk-rs-%p-%16m.profraw"

  # The maturin import hook does not treat coverage env changes as freshness
  # inputs, so force a rebuild to make the loaded extension instrumented.
  export POLY_ADK_FORCE_MATURIN_REBUILD=1
  export UV_CACHE_DIR="${UV_CACHE_DIR:-$workspace/target/uv-cache}"
  cd "$workspace/adk-python"
  uv run python -m unittest discover -s tests
)

cargo llvm-cov report --lcov --output-path "$output_dir/lcov.info"

if [[ -n "$html_dir" ]]; then
  cargo llvm-cov report --html --output-dir "$html_dir"
fi

cargo crap \
  --workspace \
  "${CRAP_EXCLUDES[@]}" \
  "${CRAP_ALLOWS[@]}" \
  --lcov "$output_dir/lcov.info" \
  --threshold "$threshold" \
  --summary
cargo crap \
  --workspace \
  "${CRAP_EXCLUDES[@]}" \
  "${CRAP_ALLOWS[@]}" \
  --lcov "$output_dir/lcov.info" \
  --threshold "$threshold" \
  --format markdown \
  --output "$output_dir/crap-full-report.md"
cargo crap \
  --workspace \
  "${CRAP_EXCLUDES[@]}" \
  "${CRAP_ALLOWS[@]}" \
  --lcov "$output_dir/lcov.info" \
  --threshold "$threshold" \
  --format json \
  --output "$output_dir/crap-report.json"

if [[ -n "$pr_comment_output" ]]; then
  PR_COMMENT_ARGS=(
    --workspace
    "${CRAP_EXCLUDES[@]}"
    "${CRAP_ALLOWS[@]}"
    --lcov "$output_dir/lcov.info"
    --threshold "$threshold"
    --format pr-comment
    --output "$pr_comment_output"
  )
  if [[ -n "$baseline" ]]; then
    PR_COMMENT_ARGS+=(--baseline "$baseline")
  fi
  if [[ -n "$repo_url" ]]; then
    PR_COMMENT_ARGS+=(--repo-url "$repo_url")
  fi
  if [[ -n "$commit_ref" ]]; then
    PR_COMMENT_ARGS+=(--commit-ref "$commit_ref")
  fi
  cargo crap "${PR_COMMENT_ARGS[@]}"
fi
