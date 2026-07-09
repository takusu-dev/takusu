#!/usr/bin/env bash
# issue-assign.sh — assign GitHub issues to the current user only if unassigned.
#
# Usage:
#   issue-assign.sh <number> [<number>...] [--assignee <user>]
#
# Assigns the given issue(s) to the specified user (default @me) only if the
# issue currently has zero assignees. This is safe for agents: it is a no-op
# when an issue is already assigned, and it reports the existing assignees.
#
# Requires: gh (authenticated), jq.

set -euo pipefail

die() { echo "issue-assign: $*" >&2; exit 1; }

# gh requires a git repo in cwd or an ancestor. This workspace is a jj
# secondary workspace that shares its git repo with another workspace, so
# .git may live elsewhere. Resolve the git toplevel via `jj git root` and
# cd there before invoking gh. Override with $TAKUSU_REPO_ROOT (abs path).
_resolve_repo_root() {
  if [ -n "${TAKUSU_REPO_ROOT:-}" ]; then
    echo "$TAKUSU_REPO_ROOT"; return
  fi
  local gitdir
  gitdir="$(jj git root 2>/dev/null || true)"
  if [ -n "$gitdir" ]; then
    dirname "$gitdir"; return
  fi
  pwd
}
cd "$(_resolve_repo_root)"

usage() {
  sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

is_tty() { [ -t 1 ]; }

assignee="@me"
numbers=()
while [ $# -gt 0 ]; do
  case "$1" in
    --assignee|-a) assignee="$2"; shift 2 ;;
    --help|-h)     usage 0 ;;
    -*)            die "unknown option: $1" ;;
    *)
      if [[ "$1" =~ ^[0-9]+$ ]]; then
        numbers+=("$1")
      else
        die "invalid issue number: $1"
      fi
      shift ;;
  esac
done

[ ${#numbers[@]} -eq 0 ] && die "missing issue number(s)"

exit_code=0
for num in "${numbers[@]}"; do
  info=$(gh issue view "$num" --json number,title,assignees 2>/dev/null) || {
    echo "issue-assign: $num: cannot fetch issue" >&2
    exit_code=1
    continue
  }

  count=$(echo "$info" | jq -r '.assignees | length')
  existing=$(echo "$info" | jq -r '[.assignees[].login] | join(",")')

  if [ "$count" -eq 0 ]; then
    if gh issue edit "$num" --add-assignee "$assignee" >/dev/null 2>&1; then
      if is_tty; then
        echo "Assigned #${num} to ${assignee}"
      else
        echo -e "${num}\t${assignee}\tassigned"
      fi
    else
      echo "issue-assign: $num: failed to assign" >&2
      exit_code=1
    fi
  else
    if is_tty; then
      echo "#${num} already assigned to: ${existing}"
    else
      echo -e "${num}\t${existing}\talready-assigned"
    fi
  fi
done

exit $exit_code
