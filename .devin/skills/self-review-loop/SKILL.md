---
name: self-review-loop
description: Review the current change by giving a subagent a brief description and an example skill to follow
argument-hint: "<brief-description> [example-skill]"
allowed-tools:
  - exec
  - read
  - run_subagent
  - read_subagent
  - todo_write
  - grep
---

Review the current `jj` change (`@`).

This skill has two modes:

- **Top-level mode**: run `/self-review-loop <brief-description> [example-skill]` to spawn a subagent that reviews `@` for you.
- **Manual/checklist mode**: if you are a subagent and cannot spawn nested subagents, `read` this file and follow the "Review steps" yourself.

## Arguments
- `brief-description`: first argument, required. Short summary of what the change is intended to do.
- `example-skill`: second argument, default `code-review`. Name of a skill in `.devin/skills/` whose review instructions the subagent should use.

## Review steps (for both modes)
1. Read the change.
   - Run `jj diff -r @` to get the diff.
   - If the diff is empty, report that there is nothing to review.
2. Read the example skill.
   - `read .devin/skills/<example-skill>/SKILL.md`
   - Extract the review criteria/checklist from its body.
3. Review the change against the criteria.
   - Correctness, security, performance, style, tests, and consistency.
   - Read surrounding context for changed files if needed.
   - Cite specific file paths and line numbers for every issue or point.
   - Provide an overall assessment.

## Top-level mode only
4. Spawn a `subagent_general` subagent.
   - Title: `self-review-for-@`
   - `is_background: true`
   - Task: include the brief description, the diff, and the review criteria from the example skill. Instruct the subagent to follow the "Review steps" above and return findings with file paths and line numbers.
5. Wait for the subagent to finish using `read_subagent`.
6. Return the subagent's review verbatim, with a short summary at the top.
7. `dunstify "takusu agent" "self-review-loop finished"`
