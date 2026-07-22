#!/usr/bin/env python3
"""Differential regression test runner for Rust.

For regression PRs that add failing tests:
- Identify tests that exist in HEAD but not in the base branch.
- Run only those new tests.
- Pass (exit 0) only when every new test fails.
- Fail (exit 1) if any new test passes (it should be a failing regression test).

For fix PRs that change implementation without adding tests:
- Run tests in crates/packages that have changed files.
- Pass (exit 0) only when all those tests pass.
"""

import json
import os
import re
import subprocess
import sys
import tempfile


def run(cmd, cwd=None, env=None):
    merged_env = {**os.environ, **(env or {})}
    print(f"+ {' '.join(cmd)} (cwd={cwd or '.'})", flush=True)
    result = subprocess.run(cmd, cwd=cwd, env=merged_env, capture_output=True, text=True)
    return result


def is_sha(ref):
    return bool(re.fullmatch(r"[0-9a-f]{40}", ref or ""))


def git_ref(base_ref):
    """Return a ref usable by git commands.

    Branch names are prefixed with origin/; full SHAs are used as-is.
    """
    if base_ref.startswith("origin/") or is_sha(base_ref):
        return base_ref
    return f"origin/{base_ref}"


def list_tests(repo_dir, env):
    res = run(
        ["cargo", "nextest", "list", "--all", "--message-format", "json"],
        cwd=repo_dir,
        env=env,
    )
    if res.returncode != 0:
        print(res.stdout)
        print(res.stderr, file=sys.stderr)
        sys.exit(1)
    data = json.loads(res.stdout)
    tests = set()
    for suite in data.get("rust-suites", {}).values():
        binary_name = suite.get("binary-name", "")
        for name in suite.get("testcases", {}).keys():
            tests.add((binary_name, name))
    return tests


def parse_libtest_name(full_name):
    """Parse a libtest-json test name into (binary_name, test_name)."""
    if "$" not in full_name:
        return None, full_name
    binary_id, test_name = full_name.split("$", 1)
    binary_name = binary_id.split("::")[-1]
    return binary_name, test_name


def run_filtered_tests(repo_dir, tests_to_run, env):
    """Run a specific set of (binary_name, test_name) and return their statuses."""
    env = {**env, "NEXTEST_EXPERIMENTAL_LIBTEST_JSON": "1"}
    # Build filterset with binary disambiguation.
    expr_parts = [f"binary(={binary}) & test(={name})" for binary, name in tests_to_run]
    chunk_size = 50
    all_results = {}
    for i in range(0, len(expr_parts), chunk_size):
        chunk = expr_parts[i:i + chunk_size]
        expr = " | ".join(f"({p})" for p in chunk)
        res = run(
            ["cargo", "nextest", "run", "--filterset", expr, "--message-format", "libtest-json"],
            cwd=repo_dir,
            env=env,
        )
        for line in res.stdout.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if obj.get("type") == "test":
                binary, test_name = parse_libtest_name(obj.get("name", ""))
                if binary is None or not test_name:
                    continue
                key = (binary, test_name)
                if obj.get("event") == "ok":
                    all_results[key] = "pass"
                elif obj.get("event") == "failed":
                    all_results[key] = "fail"
        if res.returncode != 0 and not all_results:
            # No test results and command failed: likely a compile/setup error.
            print(res.stdout)
            print(res.stderr, file=sys.stderr)
            sys.exit(1)
    return all_results


def run_package_tests(repo_dir, packages, env):
    if not packages:
        return {}, 0
    cmd = ["cargo", "nextest", "run"]
    for pkg in packages:
        cmd.extend(["-p", pkg])
    env = {**env, "NEXTEST_EXPERIMENTAL_LIBTEST_JSON": "1"}
    res = run(cmd, cwd=repo_dir, env=env)
    results = {}
    for line in res.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        if obj.get("type") == "test" and obj.get("event") in ("ok", "failed"):
            binary, test_name = parse_libtest_name(obj.get("name", ""))
            if binary is None or not test_name:
                continue
            results[(binary, test_name)] = "pass" if obj["event"] == "ok" else "fail"
    if res.returncode != 0 and not results:
        print(res.stdout)
        print(res.stderr, file=sys.stderr)
        sys.exit(1)
    return results


RUST_ROOT_FILES = {
    "Cargo.toml",
    "Cargo.lock",
    "flake.nix",
    "flake.lock",
    "rust-toolchain.toml",
}


def changed_packages(repo_dir, base_ref):
    res = run(["git", "diff", "--name-only", f"{base_ref}...HEAD"], cwd=repo_dir)
    if res.returncode != 0:
        print(res.stderr, file=sys.stderr)
        sys.exit(1)
    crates = set()
    has_workspace_change = False
    for line in res.stdout.splitlines():
        if line.startswith("crates/"):
            parts = line.split("/")
            if len(parts) >= 2:
                crates.add(parts[1])
        elif line and not line.startswith(".") and "/" not in line:
            # Top-level Rust/Nix files affect the whole workspace.
            if os.path.basename(line) in RUST_ROOT_FILES:
                has_workspace_change = True
    return crates, has_workspace_change


def all_packages(repo_dir):
    """Return all workspace member package names from Cargo metadata."""
    res = run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=repo_dir)
    if res.returncode != 0:
        print(res.stderr, file=sys.stderr)
        sys.exit(1)
    data = json.loads(res.stdout)
    packages = {pkg["name"] for pkg in data.get("packages", []) if pkg.get("name")}
    return packages


def determine_base_ref():
    event = os.environ.get("GITHUB_EVENT_NAME", "")
    if event == "pull_request":
        return os.environ.get("GITHUB_BASE_REF", "main")
    if event == "push":
        before = os.environ.get("GITHUB_EVENT_BEFORE", "")
        if before and before != "0000000000000000000000000000000000000000":
            return before
    return "main"


def main():
    head_dir = os.environ.get("GITHUB_WORKSPACE", os.getcwd())
    base_ref = determine_base_ref()
    resolved_base = git_ref(base_ref)

    # Share target directory between base and head to reuse rust-cache artifacts.
    target_dir = os.environ.get("CARGO_TARGET_DIR") or os.path.join(head_dir, "target")
    base_env = {"CARGO_TARGET_DIR": target_dir}

    # Fetch branch base refs; SHAs are already present with fetch-depth: 0.
    if not is_sha(base_ref):
        run(["git", "fetch", "origin", base_ref], cwd=head_dir)

    # Create a worktree for the base so HEAD and base can be built independently.
    base_dir = tempfile.mkdtemp(prefix="takusu-base-")
    res = run(["git", "worktree", "add", base_dir, resolved_base], cwd=head_dir)
    if res.returncode != 0:
        print(res.stderr, file=sys.stderr)
        sys.exit(1)

    try:
        base_tests = list_tests(base_dir, base_env)
        head_tests = list_tests(head_dir, base_env)

        new_tests = head_tests - base_tests
        print(f"base tests: {len(base_tests)}, head tests: {len(head_tests)}, new tests: {len(new_tests)}", flush=True)

        if new_tests:
            # Regression PR: run only the newly added tests and expect them all to fail.
            results = run_filtered_tests(head_dir, new_tests, base_env)

            missing = new_tests - set(results.keys())
            if missing:
                print(f"warning: {len(missing)} new tests were not executed", flush=True)

            passed = {t for t, status in results.items() if status == "pass"}
            failed = {t for t, status in results.items() if status == "fail"}

            if passed:
                print("error: the following newly added tests passed, but they should fail:", file=sys.stderr)
                for binary, name in sorted(passed):
                    print(f"  - {binary} {name}", file=sys.stderr)
                sys.exit(1)

            actually_failed = failed & new_tests
            if not actually_failed:
                print("error: no newly added test failed; regression tests should demonstrate a bug", file=sys.stderr)
                sys.exit(1)

            print(f"ok: all {len(actually_failed)} newly added regression test(s) failed as expected")
            sys.exit(0)
        else:
            # Fix PR (or no test change): run tests in crates/packages touched by the diff.
            crates, has_workspace_change = changed_packages(head_dir, resolved_base)
            if has_workspace_change:
                packages = all_packages(head_dir)
            elif crates:
                # Map crate directory names to package names. Cargo's -p accepts package names,
                # which usually match the crate directory name; verify with metadata.
                all_pkgs = all_packages(head_dir)
                packages = {pkg for pkg in all_pkgs if pkg.replace("-", "_") in crates or pkg in crates}
                if not packages:
                    packages = all_pkgs
            else:
                print("no code changes detected; nothing to test")
                sys.exit(0)

            print(f"running tests in packages: {', '.join(sorted(packages))}", flush=True)
            results = run_package_tests(head_dir, packages, base_env)
            failed = [t for t, status in results.items() if status == "fail"]
            if failed:
                print("error: the following tests failed:", file=sys.stderr)
                for binary, name in sorted(failed):
                    print(f"  - {binary} {name}", file=sys.stderr)
                sys.exit(1)

            print("ok: all tests passed in changed packages")
            sys.exit(0)
    finally:
        run(["git", "worktree", "remove", "--force", base_dir], cwd=head_dir)


if __name__ == "__main__":
    main()
