---
name: regression-fix
description: Spawn agents to fix regression issues in a shared working branch and open a single PR to the integration branch
argument-hint: "<branch> [issue1 issue2 ... | --max N]"
allowed-tools:
  - exec
  - read
  - run_subagent
  - read_subagent
  - todo_write
  - grep
---

Spawn multiple subagents that each fix a regression issue on `<branch>` in a shared working change. All fixes are accumulated into one commit and submitted as a single PR to `<branch>`.

## Arguments
- `branch`: first argument, required. Integration branch name (e.g. `regression-780`).
- Remaining arguments:
  - A list of issue numbers: fix only those.
  - `--max N`: from the auto-detected regression issues, fix only the first N.
  - Nothing: fix all open auto-detected regression issues.

## Steps
1. Use `todo_write` to track progress.
2. Abort if `branch` is missing.
3. Run `jj git fetch`.
4. Identify and claim target issues.
   - If issue numbers are provided, use them.
   - Otherwise try to list open sub-issues of the parent issue:
     `gh issue view <issue-number> --json subIssues --jq '.subIssues.nodes[] | select(.state == "OPEN") | .number'`
     - If this returns at least one number, use those.
   - Fallback:
     `gh issue list --state open --search "\"<branch>\"" --json number,title,body --limit 100`
     - Select issues whose body contains `<branch>`.
   - For each candidate issue, run `./scripts/issue-assign.sh <number>` to claim it.
     - Keep only issues that are assigned to you.
     - If an issue is already assigned to someone else, exclude it and report.
   - If `--max N` is given, limit to N issues.
5. Create a single shared working change on `<branch>`.
   - `jj git fetch`
   - `jj new <branch>` (or `jj new origin/<branch>` if the local bookmark is missing).
   - Verify the working copy is empty with `jj diff --summary -r @`.
   - Create one bookmark for the whole fix group:
     `bookmark_name="regression-<issue-number>-fix-$(date +%s%N)"`
     `jj bookmark create "$bookmark_name"`
   - Do not push yet.
6. Spawn subagents serially, one per target issue.
   For each `<target_issue_number>`:
   - `jj git fetch`
   - If the `<branch>` bookmark has moved since the working copy was created, rebase the working copy onto it: `jj rebase -r @ -d <branch>`. Resolve any conflicts before spawning the subagent; if you cannot resolve cleanly, report to the user and stop.
   - Spawn one `subagent_general` subagent with `is_background: false`.
     - Title: `regression-fix-<target_issue_number>`
     - Task: the "fix subagent task" below, substituting `<bookmark-name>`, `<branch>`, and `<issue_number>` with the real values.
   - Collect the result before starting the next subagent.
7. Finalize the combined commit.
   - `jj git fetch`
   - If `<branch>` has moved, `jj rebase -r @ -d <branch>` so the final change sits on top of the latest integration branch.
   - Run `cargo fmt --all` (or `treefmt`) to clean up formatting across all touched crates.
   - Run `cargo clippy` for each crate touched by the combined diff. A quick way is to run `cargo clippy --all-targets --all-features -- -D warnings`, but if that is too slow, run `cargo clippy -p <crate>` for each crate seen in `jj diff --summary -r @`.
   - `jj describe` the working copy with a combined message, e.g.:
     `jj describe -m "fix: resolve regression sub-issues for #<issue-number>"`
     If any subagent provided short summaries, include them in the body.
8. Create a single PR.
   - Build the PR body with one `Closes #<issue>` line for each fixed sub-issue.
   - `jj git push --bookmark <bookmark-name>`
   - `gh pr create --base <branch> --head <bookmark-name> --title "fix: resolve regression sub-issues for #<issue-number>" --body "Closes #<n1>\nCloses #<n2>\n...\n\nAfter merging into \`<branch>\`, the fixes will reach main through the integration PR for issue #<issue-number>."`
9. Report to the user and `dunstify "takusu agent" "regression-fix finished: single PR for #<issue-number>"`.

## fix subagent task

You are a fixer for takusu. Fix issue #<issue_number> in the shared working change on `<branch>`.

1. Understand the issue
   - Run `gh issue view <issue_number>` to read the issue body.
   - If the issue links to a regression PR, view it with `gh pr view <pr>` / `gh pr diff <pr>` to see the added test.
   - `read` the added test file and understand why it fails.

2. Setup
   - The parent has created a single shared working change on `<branch>` and set the bookmark `<bookmark-name>`. Multiple subagents are editing this same change.
   - **Warning: the working copy may already contain changes from other subagents.** Run `jj diff --summary -r @` to inspect existing changes. Do not overwrite or discard them unless they directly conflict with your fix. If you encounter conflicts, resolve them with the smallest reasonable change and report any unresolved conflicts to the parent.
   - Do not run `jj new` or `jj git fetch`. Do not create or move the bookmark. Do not create a PR.

3. Fix
   - Fix the root cause with the smallest reasonable change.
   - Run `cargo check -p <crate>` / `cargo clippy -p <crate>` / `cargo fmt` as needed.

4. Verify the regression test passes
   - Run `cargo nextest run -p <crate> -- <test_name>` and confirm the target test **passes**.
   - Run any nearby tests to make sure nothing else is broken.

5. Self-review
   - `read` `.devin/skills/self-review-loop/SKILL.md` and `.devin/skills/code-review/SKILL.md`.
   - Because you are a subagent, you cannot spawn a nested subagent. Use the `self-review-loop` "Review steps" manually and skip the "Top-level mode only" section. Use the brief description of your fix and the `code-review` criteria as the example skill.
   - If you find issues, amend the change.

6. Do not commit or push
   - Do **not** run `jj describe` or `jj git push`. The parent will compose one commit from all subagent changes.
   - If you already ran `jj describe` by mistake, tell the parent so it can be squashed or reset.

7. Report
   - Return the fixed issue number, a one-line summary of the fix, the test name that passed, the path to the self-review-loop skill, and any unresolved conflicts or warnings.
