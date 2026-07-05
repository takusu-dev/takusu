#!/usr/bin/env bash
# discord-notify.sh — send a message (or embed) to a Discord webhook.
#
# Usage:
#   discord-notify.sh "plain text message"
#   discord-notify.sh --title "T" --desc "D" [--color 0xRRGGBB] [--url <link>]
#   discord-notify.sh --json <payload.json>          # send a raw payload
#   discord-notify.sh --stdin                        # read JSON payload from stdin
#
# The webhook URL is read from $DISCORD_WEBHOOK_URL. Set it in .envrc or your
# shell environment. The script never prints the URL. To silence the success
# echo, pass --quiet.
#
# Requires: curl, jq.
#
# Setup (one-time, in .envrc or ~/.zshrc):
#   export DISCORD_WEBHOOK_URL='https://discord.com/api/webhooks/<id>/<token>'
#
# Agent note: this script is safe to call from any directory. It does not
# require a repo. It does not read any repo state.

set -euo pipefail

die() { echo "discord-notify: $*" >&2; exit 1; }

usage() {
  sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

# Convert a color spec to a decimal integer for Discord's `color` field.
# Accepts: 0xRRGGBB, #RRGGBB, RRGGBB, or a bare decimal.
normalize_color() {
  local c="$1"
  case "$c" in
    0x*|0X*) c="${c#0[xX]}"; printf '%d' "0x$c" 2>/dev/null ;;
    \#*)     printf '%d' "0x${c#\#}" 2>/dev/null ;;
    [0-9A-Fa-f]*)
      # If it contains only hex digits and is 6 chars, treat as hex; else
      # decimal (covers bare decimals like "255" that start with a digit).
      if [[ "$c" =~ ^[0-9A-Fa-f]{6}$ ]] && [ "${#c}" = 6 ]; then
        printf '%d' "0x$c" 2>/dev/null
      else
        printf '%d' "$c" 2>/dev/null
      fi ;;
    *) return 1 ;;
  esac
}

[ -n "${DISCORD_WEBHOOK_URL:-}" ] || die "DISCORD_WEBHOOK_URL is not set"

quiet=0
title="" desc="" color="" url=""
json_file="" from_stdin=0
positional=()

while [ $# -gt 0 ]; do
  case "$1" in
    --title)    title="$2"; shift 2 ;;
    --desc)     desc="$2"; shift 2 ;;
    --color)    color="$2"; shift 2 ;;
    --url)      url="$2"; shift 2 ;;
    --json)     json_file="$2"; shift 2 ;;
    --stdin)    from_stdin=1; shift ;;
    --quiet|-q) quiet=1; shift ;;
    --help|-h)  usage 0 ;;
    --)         shift; while [ $# -gt 0 ]; do positional+=("$1"); shift; done ;;
    -*)         die "unknown flag: $1" ;;
    *)          positional+=("$1"); shift ;;
  esac
done

build_payload() {
  if [ "$from_stdin" = "1" ]; then
    cat
    return
  fi
  if [ -n "$json_file" ]; then
    [ -f "$json_file" ] || die "json file not found: $json_file"
    cat "$json_file"
    return
  fi
  if [ -n "$title" ] || [ -n "$desc" ]; then
    # Build the embed object with jq, passing color as a decimal number.
    # Accepts "0xRRGGBB", "#RRGGBB", or a bare decimal.
    # All string values (title, desc, url) go through jq --arg for proper
    # JSON escaping — never splice them raw into the filter string.
    local color_arg=() url_arg=()
    if [ -n "$color" ]; then
      local dec
      dec="$(normalize_color "$color")" || die "invalid --color: $color (use 0xRRGGBB, #RRGGBB, or decimal)"
      color_arg=(--argjson color "$dec")
    fi
    [ -n "$url" ] && url_arg=(--arg u "$url")
    jq -nc --arg t "$title" --arg d "$desc" "${color_arg[@]}" "${url_arg[@]}" \
      '{embeds: [{title: $t, description: $d'"${color:+, color: \$color}""${url:+, url: \$u}"'}]}'
    return
  fi
  if [ "${#positional[@]}" -gt 0 ]; then
    jq -nc --arg c "${positional[*]}" '{content: $c}'
    return
  fi
  die "no message provided (pass text, --title/--desc, --json, or --stdin)"
}

payload="$(build_payload)"

# Validate JSON shape minimally.
echo "$payload" | jq -e '.content // .embeds // empty' >/dev/null \
  || die "payload must contain content or embeds"

# Send. Use --fail-with-body so curl returns non-zero on HTTP 4xx/5xx
# (Discord errors: 400 bad payload, 401 bad webhook, 404 unknown webhook,
# 429 rate limited) while still capturing the response body for the error
# message. Capture the exit code separately from stderr so a transport
# failure (DNS, connection refused, timeout) is also caught.
rc=0
response="$(curl -sS --fail-with-body -m 10 -X POST \
  -H 'Content-Type: application/json' \
  -d "$payload" "$DISCORD_WEBHOOK_URL" 2>&1)" || rc=$?

if [ "$rc" -ne 0 ]; then
  die "discord send failed (exit $rc): $response"
fi

# Success. Discord returns 204 No Content with empty body, or 200 with a
# body for some webhook types.
if [ -z "$response" ]; then
  [ "$quiet" = "1" ] || echo "discord: sent"
  exit 0
fi

[ "$quiet" = "1" ] || echo "discord: sent"
exit 0
