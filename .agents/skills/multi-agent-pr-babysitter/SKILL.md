---
name: multi-agent-pr-babysitter
description: Coordinate parallel subagents that each create feature branches and GitHub pull requests, then act as the main agent who reviews, watches CI, marks PRs ready for review, merges low-risk PRs when authorized, and consults the human user on higher-risk changes. Use when the user asks for multi-agent work, subagents opening PRs, PR babysitting, CI babysitting, merge sequencing, stacked integration checks, or end-to-end handling of several related GitHub PRs.
---

# Multi-Agent PR Babysitter

## Overview

Use this workflow only after the user explicitly authorizes subagents or parallel agent work. The main agent owns coordination: decompose work, spawn bounded workers, review their PRs, watch checks, test branch interactions, and make the readiness or merge decision.

## Worker Setup

- Split work into independent scopes with disjoint write ownership.
- Give every worker a branch name, PR title, changed-file ownership, and required checks.
- Tell workers they are not alone in the repo: do not revert unrelated edits, keep changes mechanically scoped, and adapt to nearby work instead of clobbering it.
- Prefer each worker opening a draft PR. Ask for a final report with PR URL, branch, changed paths, checks run, and known risks.
- Keep doing non-overlapping main-agent work while workers run. Wait only when the next critical step needs a worker result.

## Main-Agent Review

- Inspect each PR directly; do not rely only on worker summaries.
- Check changed files, review the diff, and verify GitHub checks.
- For multiple PRs, test the combined stack in a temporary clone or worktree before marking anything merge-ready.
- Preserve the user's local dirty worktree. Avoid switching branches in the shared checkout unless necessary; prefer GitHub diffs, temporary clones, or worktrees.
- Sequence PRs by independence and conflict risk. Prefer merging clean, orthogonal PRs first.

## Ready And Merge Decisions

- If the user only asked to open PRs or wants human review, leave PRs as draft and summarize readiness.
- If the user asked to babysit, merge, or handle the workflow end to end, do not stop at "checks passed." Review the PRs and take the next appropriate action.
- Mark a PR ready for review when it is low risk, scoped as requested, locally reviewed by the main agent, has passing required checks, and does not depend on an unmerged sibling PR.
- Merge low-risk PRs when the user has delegated merging authority and branch protection allows it.
- Ask the human user before marking ready or merging when risk is moderate or high.

Low-risk examples: docs-only changes, mechanical code moves with passing full tests, narrow test additions, isolated refactors with no behavior change, or one-file fixes with clear coverage.

Higher-risk examples: behavior changes, release/versioning changes, migrations, broad API changes, generated artifacts, security/auth changes, destructive data/file operations, large overlapping refactors, flaky or incomplete checks, unresolved review comments, or anything the main agent cannot confidently explain.

## CI Babysitting

- Watch required checks until completion when the user asks for babysitting.
- If CI fails, inspect logs, identify whether the failure is relevant or flaky, and fix relevant failures on the PR branch.
- Re-run failed jobs only when appropriate. Do not repeatedly re-run without diagnosis.
- After checks pass, refresh mergeability and review state before marking ready or merging.

## Final Report

Report PR URLs, status, checks, merge/readiness actions taken, and any deferred risks. If PRs were merged, include the merge order. If you stopped for human input, state the exact decision needed.
