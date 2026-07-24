---
name: regression-init
description: Create a regression integration branch and a draft PR targeting main
argument-hint: "[issue-number] [branch-name]"
allowed-tools:
  - exec
  - read
---

Create a regression integration branch rooted on `main` and open a draft PR against `main`.

## Arguments
- `issue-number`: first argument, default `780`.
- `branch-name`: second argument, default `regression-{issue-number}`.

## Prerequisites
- The repo is managed with `jj`.
- `jj st` / `jj git fetch` / `gh` / `dunstify` are available.
- Follow the commit message conventions from `.devin/rules/pr-workflow.md`.

## Steps
1. Check the working copy for uncommitted changes.
   - Run `jj diff --summary -r @`.
   - If the output is not empty, abort and report to the user (optionally `dunstify "takusu agent" "regression-init aborted: working copy is not clean"`).

2. Run `jj git fetch`.

3. Create the integration change.
   - `jj new origin/main -m "plan: create regression integration branch for #<issue-number>"`
   - Use a lowercase first word and no trailing period.

4. Push the integration branch.
   - `jj git push --named <branch-name>=@`
   - This creates `origin/<branch-name>`.

5. Check for an existing PR.
   - `gh pr list --head <branch-name> --state open --json number,url`
   - If one exists, report its URL and skip to step 7.

6. Create the PR.
   - `gh pr create --base main --head <branch-name> --draft --title "plan: debug #<issue-number>" --body "Integration branch for the debug plan of #<issue-number>.\nCollects failing regression tests and their fixes.\nDo not merge until all regression tests pass."`
   - Capture the PR URL.

7. Report the result.
   - State the branch name and the PR URL (if created).
   - `dunstify "takusu agent" "regression-init finished: <branch-name> / <pr-url>"`
