#!/usr/bin/env bash
# issue-view.sh — list and inspect GitHub issues for the current repo.
#
# Usage:
#   issue-view.sh list [--label <label>] [--assignee <user|@me|unassigned>]
#                      [--state <open|closed|all>] [--limit <N>] [--search <query>]
#   issue-view.sh show <number>
#
# Designed for agent use: deterministic, plain-text output, no color unless
# stdout is a TTY. Wraps `gh issue list` / `gh issue view` so the agent does
# not have to remember the exact flag spellings.
#
# Requires: gh (authenticated), jq for show formatting.

set -euo pipefail

die() { echo "issue-view: $*" >&2; exit 1; }

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

is_tty() { [ -t 1 ]; }

cmd_list() {
  local label="" assignee="" state="open" limit=30 search=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --label|-l)    label="$2"; shift 2 ;;
      --assignee|-a) assignee="$2"; shift 2 ;;
      --state|-s)    state="$2"; shift 2 ;;
      --limit|-n)    limit="$2"; shift 2 ;;
      --search|-q)   search="$2"; shift 2 ;;
      --help|-h)     usage 0 ;;
      *)             die "list: unknown arg: $1" ;;
    esac
  done

  local args=(--state "$state" --limit "$limit")
  [ -n "$label" ] && args+=(--label "$label")
  # Handle --assignee specially: "unassigned" is not a real GitHub user,
  # so use the search syntax 'no:assignee' instead of --assignee.
  local search_terms=()
  [ -n "$search" ] && search_terms+=("$search")
  if [ -n "$assignee" ]; then
    if [ "$assignee" = "unassigned" ]; then
      search_terms+=("no:assignee")
    else
      args+=(--assignee "$assignee")
    fi
  fi
  if [ "${#search_terms[@]}" -gt 0 ]; then
    args+=(--search "${search_terms[*]}")
  fi

  if is_tty; then
    gh issue list "${args[@]}"
  else
    # Plain text for agent consumption: number\ttitle\tlabels\tassignees\tstate
    gh issue list "${args[@]}" --json number,title,labels,assignees,state \
      | jq -r '.[] | [.number, .title,
                      ([.labels[].name] | join(",")),
                      ([.assignees[].login] | join(",")),
                      .state] | @tsv'
  fi
}

cmd_show() {
  [ $# -ge 1 ] || die "show: missing issue number"
  local num="$1"; shift
  while [ $# -gt 0 ]; do
    case "$1" in
      --help|-h) usage 0 ;;
      *) die "show: unknown arg: $1" ;;
    esac
  done

  if is_tty; then
    gh issue view "$num" --comments
  else
    # Structured: title, body, then comments as a thread.
    gh issue view "$num" --json number,title,body,labels,assignees,state,comments \
      | jq -r '
        "#\(.number) \(.title) [\(.state)]",
        "labels:    \([.labels[].name] | join(", "))",
        "assignees: \([.assignees[].login] | join(", "))",
        "",
        .body,
        "",
        ("--- comments (\(.comments | length)) ---"),
        (.comments[] | "",
                      "## \(.author.login) — \(.createdAt)",
                      .body)
      '
  fi
}

[ $# -ge 1 ] || usage 1
sub="$1"; shift
case "$sub" in
  list)  cmd_list "$@" ;;
  show)  cmd_show "$@" ;;
  -h|--help) usage 0 ;;
  *)     die "unknown subcommand: $sub (expected: list|show)" ;;
esac
