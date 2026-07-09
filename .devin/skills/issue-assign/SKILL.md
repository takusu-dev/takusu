---
name: issue-assign
description: Assign a GitHub issue to the current user only if it is currently unassigned
argument-hint: "<number> [<number>...] [--assignee <user>]"
allowed-tools:
  - exec
  - read
---

Use the `scripts/issue-assign.sh` helper to assign GitHub issues. The helper
first checks that an issue has zero assignees, then adds the assignee. This
avoids taking over an issue that someone else is already handling.

## Commands

- `./scripts/issue-assign.sh <number>`
  - Assign issue `<number>` to `@me` (the current authenticated user) only if
    it has no assignees.
- `./scripts/issue-assign.sh <number> --assignee <user>`
  - Assign to a specific user instead of `@me`.
- `./scripts/issue-assign.sh <number1> <number2> ...`
  - Assign multiple issues in one call.

## Output

- In a terminal: human-readable messages.
- When stdout is not a TTY: TSV `number\tassignee(s)\tstatus`, where `status`
  is `assigned` or `already-assigned`.

## When to use

- Before starting work on a new issue, to claim it for yourself.
- To verify-and-assign an issue after the user has asked you to work on it.

## Examples

```
./scripts/issue-assign.sh 416
./scripts/issue-assign.sh 413 414 --assignee satler-git
```
