---
name: regression-discover
description: Spawn agents to add failing regression tests as PRs and create issues on the integration branch
argument-hint: "<branch> [count] [issue-number]"
allowed-tools:
  - exec
  - read
  - run_subagent
  - read_subagent
  - todo_write
  - grep
---

Spawn multiple subagents that each add a failing regression test as a PR to `<branch>` and create a corresponding issue.

## Arguments
- `branch`: first argument, required. Integration branch name (e.g. `regression-780`).
- `count`: second argument, default `4`. Number of subagents to spawn (3-6 recommended).
- `issue-number`: third argument, default `780`. Parent issue number.

## Steps
1. Use `todo_write` to track progress.
2. Abort if `branch` is missing.
3. Run `jj git fetch`.
4. Verify the integration branch exists.
   - `jj bookmark list` or `jj log -r <branch>`.
   - If the local bookmark is missing, use `origin/<branch>`.
5. Spawn `<count>` subagents serially.
   For `i` from 1 to `<count>`:
   - `jj git fetch`
   - Create a fresh working change on `<branch>`: `jj new <branch>` (or `jj new origin/<branch>` if the local bookmark is missing).
   - Verify the working copy is empty with `jj diff --summary -r @`.
   - Create a unique bookmark for the subagent to push:
     `bookmark_name="regression-<issue-number>-discover-{i}-$(date +%s%N)"`
     `jj bookmark create "$bookmark_name"`
     - Pass `$bookmark_name` as `<bookmark-name>` to the subagent task.
   - Spawn one `subagent_general` subagent with `is_background: false` (it will block until finished).
     - Title: `regression-discover-{i}`
     - Task: the "discover subagent task" below, substituting `<bookmark-name>` with the bookmark just created.
   - Collect the result before starting the next subagent.
6. Aggregate results.
   - List the issue numbers, PR URLs, and test file paths reported by each subagent.
   - Record any failures.
7. Report to the user and `dunstify "takusu agent" "regression-discover finished: see summary"`.

## discover subagent task

You are a debugger for takusu. For the integration branch `<branch>` (parent issue #<issue-number>), perform one cycle of the following. All changes should go through a PR targeting `<branch>`.

1. Setup
   - The parent has already created a working change on `<branch>` and set the bookmark `<bookmark-name>`.
   - Verify the working copy is clean with `jj diff --summary -r @`.
   - Do not run `jj new` or `jj git fetch`.

2. Find a bug
   - Explore the codebase (mostly under `crates/`) and find one real bug, suspicious behavior, or uncovered edge case.
   - Candidates: `TODO`/`FIXME`/`unimplemented!` comments, failing existing `examples/` or tests, odd behavior in planner score functions, datetime/rrule/ical corner cases.
   - Identify the minimal reproduction conditions.

3. Add a failing regression test
   - Add a test in an existing test file or a new `tests/` file that demonstrates the bug.
   - Name it clearly, e.g. `regression_...`.
   - Run `cargo nextest run -p <crate> -- <test_name>` and confirm the test **fails**. If it does not, reconsider the bug or pick another issue.

4. Commit
   - `jj describe -m "regression: <short description> for #<issue-number>"`
   - Lowercase first word, no trailing period.

5. Create a PR
   - `jj git push --bookmark <bookmark-name>`
   - `gh pr create --base <branch> --head <bookmark-name> --title "regression: <...>" --body "Adds a failing regression test for <...>\n\nRelated to issue #<issue-number>.\nTarget branch: \`<branch>\`.\nDo not merge until the fix is provided."`

6. Avoid duplicate issues
   - `gh issue list --state open --search "\"<branch>\"" --json number,title,body --limit 100`
   - Filter for issues whose title or body contains the regression test name or the bug summary.
   - If a matching issue is found, reuse its number and skip to step 8.

7. Create an issue
   - Try with `--parent` first:
     `gh issue create --parent <issue-number> --title "bug: <...>" --body "<detailed description>\n\nThe failing regression test was added in #<pr-number> (branch \`<branch>\`).\nPlease fix this with a PR targeting \`<branch>\`. Do not open a PR directly to main."`
   - If that fails because `--parent` is unsupported, retry without `--parent` and put "Sub-issue of #<issue-number>" at the top of the body:
     `gh issue create --title "bug: <...>" --body "Sub-issue of #<issue-number>.\n\n<detailed description>\n\nThe failing regression test was added in #<pr-number> (branch \`<branch>\`).\nPlease fix this with a PR targeting \`<branch>\`. Do not open a PR directly to main."`
   - Add the `regression` label to the newly created issue:
     `gh issue edit <issue-number> --add-label regression`

8. Report
   - Return the issue number (new or reused), PR URL, test file path, and a short summary of the failure message.
