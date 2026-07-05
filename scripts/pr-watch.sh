#!/usr/bin/env bash
# pr-watch.sh — inspect and watch a GitHub pull request for changes.
#
# Usage:
#   pr-watch.sh show <PR>            Print a one-shot snapshot of the PR.
#   pr-watch.sh watch <PR> [--interval N] [--max N]
#                                    Loop and print diffs from the previous
#                                    snapshot to stdout. No desktop/Discord
#                                    notification — the agent reads stdout.
#                                    Default interval: 60s. --max limits the
#                                    number of iterations (0 = unlimited).
#
# Snapshot covers: CI checks, reviews, and issue comments. The watch loop
# hashes each section and only re-emits sections whose content changed.
#
# Requires: gh (authenticated), jq.

set -euo pipefail

die() { echo "pr-watch: $*" >&2; exit 1; }

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
  sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

# Snapshot a PR as a JSON document with three sections.
# Sections are kept as raw strings so we can diff them verbatim.
snapshot() {
  local pr="$1"
  gh pr view "$pr" --json number,title,state,mergeable,mergeStateStatus,statusCheckRollup,reviewDecision,reviews,comments \
    | jq -c '
      {
        header: ("#\(.number) \(.title) [\(.state)] mergeable=\(.mergeable // "?") status=\(.mergeStateStatus // "?") review=\(.reviewDecision // "?")"),
        checks: ([.statusCheckRollup[]? |
                  "\(.name // .context // "?"): \(.state // .status // "?")\(if .conclusion then " (\(.conclusion))" else "" end)"]
                  | sort | join("\n")),
        reviews: ([.reviews[]? |
                   "\(.author.login) \(.state) @\(.submittedAt)"]
                   | sort | join("\n")),
        comments: ([.comments[]? |
                    "\(.author.login) @\(.createdAt): \(.body[0:200])"]
                    | sort | join("\n"))
      }'
}

emit_snapshot() {
  local snap="$1"
  echo "$snap" | jq -r '
    .header, "",
    "## checks", .checks, "",
    "## reviews", .reviews, "",
    "## comments", .comments, ""
  '
}

emit_diff() {
  local prev="$1" cur="$2"
  echo "=== pr-watch diff @ $(date -Iseconds) ==="
  for field in checks reviews comments; do
    local a b
    a=$(echo "$prev" | jq -r ".$field")
    b=$(echo "$cur"  | jq -r ".$field")
    if [ "$a" != "$b" ]; then
      echo "--- $field changed ---"
      echo "<<< before"
      echo "$a"
      echo ">>> after"
      echo "$b"
      echo
    fi
  done
  local h1 h2
  h1=$(echo "$prev" | jq -r '.header')
  h2=$(echo "$cur"  | jq -r '.header')
  # Guard with `|| true` so a non-zero return from `[ ]` (when headers are
  # equal) does not become the function's exit code under `set -e`.
  [ "$h1" != "$h2" ] && { echo "--- header changed ---"; echo "$h1"; echo "$h2"; } || true
}

cmd_show() {
  [ $# -ge 1 ] || die "show: missing PR number"
  local pr="$1"; shift
  while [ $# -gt 0 ]; do
    case "$1" in
      --help|-h) usage 0 ;;
      *) die "show: unknown arg: $1" ;;
    esac
  done
  emit_snapshot "$(snapshot "$pr")"
}

cmd_watch() {
  [ $# -ge 1 ] || die "watch: missing PR number"
  local pr="$1"; shift
  local interval=60 max=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --interval|-i) interval="$2"; shift 2 ;;
      --max|-m)      max="$2"; shift 2 ;;
      --help|-h)     usage 0 ;;
      *) die "watch: unknown arg: $1" ;;
    esac
  done

  local prev cur iter=0
  prev="$(snapshot "$pr")"
  echo "=== pr-watch initial @ $(date -Iseconds) ==="
  emit_snapshot "$prev"
  while true; do
    iter=$((iter + 1))
    if [ "$max" -gt 0 ] && [ "$iter" -gt "$max" ]; then
      echo "=== pr-watch reached --max $max, stopping ==="
      break
    fi
    sleep "$interval"
    if ! cur="$(snapshot "$pr" 2>/dev/null)"; then
      echo "=== pr-watch: snapshot failed @ $(date -Iseconds), retrying ===" >&2
      continue
    fi
    if [ "$cur" != "$prev" ]; then
      emit_diff "$prev" "$cur"
      prev="$cur"
    fi
  done
}

[ $# -ge 1 ] || usage 1
sub="$1"; shift
case "$sub" in
  show)  cmd_show "$@" ;;
  watch) cmd_watch "$@" ;;
  -h|--help) usage 0 ;;
  *)     die "unknown subcommand: $sub (expected: show|watch)" ;;
esac
