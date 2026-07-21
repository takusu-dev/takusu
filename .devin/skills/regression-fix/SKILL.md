---
name: regression-fix
description: Spawn agents to fix regression issues via PRs to the integration branch
argument-hint: "<branch> [issue1 issue2 ... | --max N]"
allowed-tools:
  - exec
  - read
  - run_subagent
  - read_subagent
  - todo_write
  - grep
---

Spawn multiple subagents that each fix a regression issue on `<branch>` by opening a PR to `<branch>`.

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
5. Spawn subagents serially, one per target issue.
   For each `<target_issue_number>`:
   - `jj git fetch`
   - Create a fresh working change on `<branch>`: `jj new <branch>` (or `jj new origin/<branch>` if the local bookmark is missing).
   - Verify the working copy is empty with `jj diff --summary -r @`.
   - Create a unique bookmark for the subagent to push:
     `bookmark_name="regression-<issue-number>-fix-<target_issue_number>-$(date +%s%N)"`
     `jj bookmark create "$bookmark_name"`
     - Pass `$bookmark_name` as `<bookmark-name>` and `<target_issue_number>` as `<issue_number>` to the subagent task.
   - Spawn one `subagent_general` subagent with `is_background: false` (it will block until finished).
     - Title: `regression-fix-<target_issue_number>`
     - Task: the "fix subagent task" below.
   - Collect the result before starting the next subagent.
6. Collect results.
   - List issue numbers, PR URLs, and test outcomes.
   - Record any failures.
7. Report to the user and `dunstify`.

## fix subagent task

You are a fixer for takusu. Fix issue #<issue_number> on the integration branch `<branch>` and open a PR to `<branch>`.

1. Understand the issue
   - Run `gh issue view <issue_number>` to read the issue body.
   - If the issue links to a regression PR, view it with `gh pr view <pr>` / `gh pr diff <pr>` to see the added test.
   - `read` the added test file and understand why it fails.

2. Setup
   - The parent has already created a working change on `<branch>` and set the bookmark `<bookmark-name>`.
   - Verify the working copy is clean with `jj diff --summary -r @`.
   - Do not run `jj new` or `jj git fetch`.

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

6. Commit
   - `jj describe -m "fix: <short description> for #<issue_number>"`
   - Lowercase first word, no trailing period.

7. Create a PR
   - `jj git push --bookmark <bookmark-name>`
   - `gh pr create --base <branch> --head <bookmark-name> --title "fix: <...>" --body "Fixes #<issue_number>.\n\nAfter merging into \`<branch>\`, the fix will reach main through the integration PR for issue #<issue_number>."`

8. Report
   - Return the fixed issue number, PR URL, the test name that passed, and the path to the self-review-loop skill.
