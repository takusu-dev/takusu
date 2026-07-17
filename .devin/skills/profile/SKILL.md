---
name: profile
description: Profile a Rust target under perf and emit a flamegraph + top-function summary
argument-hint: "[--example <name>|--bin <name>] [-p <pkg>] [--freq <hz>] [-o <dir>] [-- <args>]"
allowed-tools:
  - exec
---

Use the `scripts/profile.sh` helper to profile a Rust example or binary. It
builds with frame pointers + debug info, records with `perf`, and produces an
SVG flamegraph plus human-readable top-self/top-total summaries.

## Commands

- `./scripts/profile.sh --example profile -p takusu-core`
  - Profiles the `profile` example in `crates/takusu-core`.
- `./scripts/profile.sh --bin takusu-local -p takusu-local`
  - Profiles the `takusu-local` binary.
- `./scripts/profile.sh --example daily -p takusu-core -- --some-arg`
  - Passes extra arguments after `--` to the binary.

## When to use

- When investigating performance regressions or hot functions in `takusu-core`.
- After adding new benchmarks, to validate where CPU time is spent.
- As a readable alternative to raw `perf report` (which is hard to read due to
  inlined generic Rust symbols and deep rayon stacks).

## Output

All outputs are written to `target/profile/` by default (override with `-o`):

- `flamegraph.svg` — interactive flamegraph.
- `top.txt` — combined human-readable summary.
- `top-self.txt` — self time per normalized function (sorted descending).
- `top-total.txt` — total time per normalized function (sorted descending).
- `collapsed.txt` — raw folded stacks for further analysis.

## Notes

- Requires `perf` and `inferno` (inferno-collapse-perf, inferno-flamegraph).
- If the tools are missing and `nix` is available, the script pulls them from
  nixpkgs automatically.
