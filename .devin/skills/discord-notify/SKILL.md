---
name: discord-notify
description: Send a message or embed to a Discord webhook (reads DISCORD_WEBHOOK_URL from env)
argument-hint: "[text | --title T --desc D | --json file | --stdin]"
allowed-tools:
  - exec
  - read
---

Use the `scripts/discord-notify.sh` helper to send a message to Discord. The
webhook URL is read from the `DISCORD_WEBHOOK_URL` environment variable — the
script never prints it. Set it in `.envrc` or your shell:

```sh
export DISCORD_WEBHOOK_URL='https://discord.com/api/webhooks/<id>/<token>'
```

## Commands

- `./scripts/discord-notify.sh "plain text message"`
  - Sends `{content: "..."}`.
- `./scripts/discord-notify.sh --title "T" --desc "D" [--color 0xRRGGBB] [--url <link>]`
  - Sends an embed. `--color` accepts `0xRRGGBB`, `#RRGGBB`, `RRGGBB`, or a
    decimal integer. `--url` adds a link to the embed.
- `./scripts/discord-notify.sh --json <payload.json>`
  - Sends a raw JSON payload (full Discord webhook API control).
- `./scripts/discord-notify.sh --stdin`
  - Reads the JSON payload from stdin.

Pass `--quiet` to suppress the `discord: sent` confirmation line.

## When to use

- When the user asks to "notify Discord", "send a message to Discord", or
  "ping me on Discord when X happens".
- After a task completes, if the user has set up Discord notifications.
- **Do not** use this for the in-repo `dunstify` desktop notifications —
  those are for local terminal alerts (see `.devin/rules/agent-helpers.md` "Agent Notifications").

## Examples

```
./scripts/discord-notify.sh "build green on main"
./scripts/discord-notify.sh --title "PR merged" --desc "#246 merged into main" --color 0x57F287
./scripts/discord-notify.sh --json ./payload.json
echo '{"content":"hi","username":"agent"}' | ./scripts/discord-notify.sh --stdin
```

## Setup note

If `DISCORD_WEBHOOK_URL` is not set, the script exits 1 with
`discord-notify: DISCORD_WEBHOOK_URL is not set`. Tell the user to set it in
`.envrc` (and `direnv allow`) or in their shell config.
