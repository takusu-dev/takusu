---
name: pr-watch
description: Watch a GitHub pull request for CI, review, and comment changes (one-shot snapshot or polling loop)
argument-hint: "[show|watch] <PR> [options]"
allowed-tools:
  - exec
  - read
---

Use the `scripts/pr-watch.sh` helper to monitor a pull request's CI checks,
reviews, and comments. Do not call `gh pr view` directly — the helper produces
stable, diff-friendly output.

## Commands

- `./scripts/pr-watch.sh show <PR>`
  - One-shot snapshot: header (state/mergeable/review decision), CI checks,
    reviews, and the first 200 chars of each comment.
- `./scripts/pr-watch.sh watch <PR> [--interval <seconds>] [--max <N>]`
  - Loops, printing only the sections that changed since the last snapshot.
  - Default interval: 60s. `--max 0` = unlimited.
  - **No desktop/Discord notification** — output goes to stdout only. The
    agent reads stdout to decide what to do.

## When to use

- After pushing a change, to wait for CI to go green (or red).
- When a human reviewer is expected to comment — watch for new review
  comments and reply via `gh pr comment` or the GitHub MCP server.
- To detect a merge conflict (`mergeable=CONFLICTING` in the header).

## Examples

```
./scripts/pr-watch.sh show 246
./scripts/pr-watch.sh watch 246 --interval 30 --max 20
```

## Output shape (show)

```
#246 <title> [MERGED] mergeable=UNKNOWN status=UNKNOWN review=

## checks
<check name>: <state> (<conclusion>)

## reviews
<author> <state> @<submittedAt>

## comments
<author> @<createdAt>: <body excerpt>
```

The `watch` subcommand prints `=== pr-watch diff @ <iso> ===` headers
followed by `--- <section> changed ---` blocks with `<<< before` / `>>> after`
content for each changed section.
