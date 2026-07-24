# Agent Implementation Notes

The `takusu-agent` crate is designed with reference to the agent loop and
tool-calling patterns in [pi](https://github.com/earendil-works/pi)
(`packages/agent`), by Mario Zechner, used under the MIT License. While
Takusu's runtime is written in Rust and targets a voice-assistant planner, the
following design decisions from `pi` are reviewed when implementing or
verifying the agent crate:

- Provider errors and tool failures are reported back to the model as
  recoverable context, not turn-ending exceptions.
- Tool execution is wrapped in lifecycle events (start/update/end) so the UI
  can render partial progress.
- Context window management is driven by provider `usage` tokens and by
  preserving complete tool-call/tool-result groups.
- Tool results separate `content` (model-facing text) from `details` (UI/log
  structured payload).
- `beforeToolCall`/`afterToolCall` hooks allow the application to block,
  override, or enrich tool results.

When a new agent feature is implemented, cross-check against `pi` to catch
architectural mistakes (e.g. missing abort handling, wrong error recovery path,
not updating the model with tool results in source order). Do not copy code
directly; only adapt patterns.
