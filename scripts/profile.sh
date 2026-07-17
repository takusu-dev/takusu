#!/usr/bin/env bash
# profile.sh — run a Rust target under perf and emit a flamegraph + top-function summary.
#
# Usage:
#   profile.sh --example <name> [-p <pkg>] [--freq <hz>] [-o <dir>] [-- <args>]
#   profile.sh --bin <name>     [-p <pkg>] [--freq <hz>] [-o <dir>] [-- <args>]
#
# Output:
#   <dir>/flamegraph.svg
#   <dir>/collapsed.txt
#   <dir>/top-self.txt     (self time per normalized function)
#   <dir>/top-total.txt    (total time per normalized function, incl. children)
#   <dir>/top.txt          (human-readable combined summary)
#
# Requires: cargo, perf, inferno (inferno-collapse-perf, inferno-flamegraph).
# If those tools are missing, nix is used to pull them from nixpkgs.

set -euo pipefail

die() { echo "profile: $*" >&2; exit 1; }

usage() {
  sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

target_type=""
target_name=""
pkg=""
freq=997
out_dir="target/profile"
run_args=()

while [ $# -gt 0 ]; do
  case "$1" in
    --example)   target_type=example; target_name="$2"; shift 2 ;;
    --bin)       target_type=bin;     target_name="$2"; shift 2 ;;
    -p|--package) pkg="$2"; shift 2 ;;
    --freq)      freq="$2"; shift 2 ;;
    -o|--output) out_dir="$2"; shift 2 ;;
    --help|-h)   usage 0 ;;
    --)          shift; while [ $# -gt 0 ]; do run_args+=("$1"); shift; done ;;
    -*)          die "unknown flag: $1" ;;
    *)           die "unknown argument: $1" ;;
  esac
done

[ -n "$target_type" ] || die "specify --example <name> or --bin <name>"
[ -n "$target_name" ] || die "target name is required"

build_flags=("--release")
[ -n "$pkg" ] && build_flags+=("-p" "$pkg")
case "$target_type" in
  example) build_flags+=("--example" "$target_name") ;;
  bin)     build_flags+=("--bin"     "$target_name") ;;
esac

# Ensure the profiling tools are available. Use nix if possible.
need_nix=0
for tool in perf inferno-collapse-perf inferno-flamegraph; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    need_nix=1
  fi
done
if [ "$need_nix" = 1 ]; then
  command -v nix >/dev/null 2>&1 || die "perf/inferno not found and nix is not available"
fi

_run_with_tools() {
  if [ "$need_nix" = 1 ]; then
    # Keep the current PATH so cargo/rustc from the dev shell remain available.
    nix shell nixpkgs#perf nixpkgs#inferno -c "$@"
  else
    "$@"
  fi
}

# Build with frame pointers and debug info, otherwise perf stack walking fails.
echo "profile: building ${target_type} '${target_name}' with frame pointers + debuginfo..."
RUSTFLAGS="${RUSTFLAGS:-} -C force-frame-pointers=yes" \
  CARGO_PROFILE_RELEASE_DEBUG=true \
  cargo build "${build_flags[@]}"

binary=""
case "$target_type" in
  example) binary="target/release/examples/${target_name}" ;;
  bin)     binary="target/release/${target_name}" ;;
esac
[ -x "$binary" ] || die "binary not found: $binary"

mkdir -p "$out_dir"

echo "profile: recording ${binary} at ${freq}Hz..."
_run_with_tools perf record -e cycles:u -g --call-graph=fp -F "$freq" \
  -o "$out_dir/perf.data" -- "$binary" "${run_args[@]}"

echo "profile: generating collapsed stacks and flamegraph..."
_run_with_tools sh -c "perf script -i '$out_dir/perf.data' | inferno-collapse-perf > '$out_dir/collapsed.txt'"
_run_with_tools inferno-flamegraph "$out_dir/collapsed.txt" > "$out_dir/flamegraph.svg"

echo "profile: summarizing top functions..."

# Shared awk normalizer. It strips closure environments, impl blocks, and trailing
# generic argument blocks so related symbols aggregate into readable buckets.
_normalize_awk='
function normalize(name) {
  gsub(/::\{closure[^}]*\}/, "", name)
  gsub(/\{impl#[0-9]+\}/, "", name)
  gsub(/ \[[^]]*\]$/, "", name)
  gsub(/_\[[^]]*\]$/, "", name)
  while (match(name, /<[^<>]*>$/)) {
    name = substr(name, 1, RSTART - 1)
  }
  gsub(/^ +| +$/, "", name)
  return name
}
'

_run_with_tools awk "$_normalize_awk
{
  if (match(\$0, / ([0-9]+)\$/, m)) {
    count = m[1] + 0
    line = substr(\$0, 1, RSTART - 1)
  } else {
    next
  }
  total_samples += count
  n = split(line, frames, \";\")
  for (i = 1; i <= n; i++) {
    total[normalize(frames[i])] += count
  }
  self[normalize(frames[n])] += count
}
END {
  for (fn in self)  printf \"%s\\t%f\\t%s\\n\", fn, self[fn] * 100.0 / total_samples, \"self\"
  for (fn in total) printf \"%s\\t%f\\t%s\\n\", fn, total[fn] * 100.0 / total_samples, \"total\"
}" "$out_dir/collapsed.txt" > "$out_dir/top-raw.txt"

# Produce top-self and top-total files (sorted descending, limited to 40 lines).
awk -F'\t' '$3 == "self"  { printf "%6.2f%%\t%s\n", $2, $1 }' "$out_dir/top-raw.txt" | \
  LC_ALL=C sort -t$'\t' -k1 -rn | head -40 > "$out_dir/top-self.txt"

awk -F'\t' '$3 == "total" { printf "%6.2f%%\t%s\n", $2, $1 }' "$out_dir/top-raw.txt" | \
  LC_ALL=C sort -t$'\t' -k1 -rn | head -40 > "$out_dir/top-total.txt"

# Combined human-readable summary.
{
  echo "top self functions (% of samples):"
  head -20 "$out_dir/top-self.txt"
  echo ""
  echo "top total functions (% of samples, incl. callees):"
  head -20 "$out_dir/top-total.txt"
} > "$out_dir/top.txt"

echo "profile: done. outputs in $out_dir:"
echo "  flamegraph: $out_dir/flamegraph.svg"
echo "  summary:    $out_dir/top.txt"
echo "  top self:   $out_dir/top-self.txt"
echo "  top total:  $out_dir/top-total.txt"

cat "$out_dir/top.txt"
