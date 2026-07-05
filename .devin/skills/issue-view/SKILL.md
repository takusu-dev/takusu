---
name: issue-view
description: List and inspect GitHub issues (label/assignee/state filters, single-issue view with comments)
argument-hint: "[list|show] [args]"
allowed-tools:
  - exec
  - read
---

Use the `scripts/issue-view.sh` helper to inspect GitHub issues for this repo.
Do not call `gh issue` directly — the helper gives stable, agent-friendly
plain-text output (TSV for `list`, structured markdown for `show`).

## Commands

- `./scripts/issue-view.sh list [--label <label>] [--assignee <user|@me|unassigned>] [--state <open|closed|all>] [--limit <N>] [--search <query>]`
  - Prints TSV: `number\ttitle\tlabels\tassignees\tstate`
  - Filter by label, assignee, state, or free-text search.
  - `--assignee unassigned` lists issues with no assignee.
- `./scripts/issue-view.sh show <number>`
  - Prints the issue title, body, labels, assignees, and the full comment
    thread as markdown.

## When to use

- The user asks "what issues are open?", "show me issue #N", "what's assigned
  to me?", "list issues labeled `agent`".
- Before starting work, to read the full issue context (body + comments).
- After pushing a change, to check whether an issue was closed.

## Examples

```
./scripts/issue-view.sh list --label agent --limit 20
./scripts/issue-view.sh list --assignee @me --state open
./scripts/issue-view.sh show 157
```
