#!/usr/bin/env python3
"""Differential regression test runner for Jest in mobile/."""

import json
import os
import re
import subprocess
import sys


def run(cmd, cwd=None, env=None):
    merged_env = {**os.environ, **(env or {})}
    print(f"+ {' '.join(cmd)} (cwd={cwd or '.'})", flush=True)
    result = subprocess.run(cmd, cwd=cwd, env=merged_env, capture_output=True, text=True)
    return result


def is_sha(ref):
    return bool(re.fullmatch(r"[0-9a-f]{40}", ref or ""))


def git_ref(base_ref):
    """Return a ref usable by git commands."""
    if base_ref.startswith("origin/") or is_sha(base_ref):
        return base_ref
    return f"origin/{base_ref}"


def determine_base_ref():
    event = os.environ.get("GITHUB_EVENT_NAME", "")
    if event == "pull_request":
        return os.environ.get("GITHUB_BASE_REF", "main")
    if event == "push":
        before = os.environ.get("GITHUB_EVENT_BEFORE", "")
        if before and before != "0000000000000000000000000000000000000000":
            return before
    return "main"


def changed_files(repo_dir, base_ref, subdir="mobile"):
    resolved = git_ref(base_ref)
    res = run(["git", "diff", "--name-only", f"{resolved}...HEAD", "--", subdir], cwd=repo_dir)
    if res.returncode != 0:
        print(res.stderr, file=sys.stderr)
        sys.exit(1)
    return [line.strip() for line in res.stdout.splitlines() if line.strip()]


def new_test_blocks_in_diff(repo_dir, base_ref, file_path):
    """Return test titles that are added in the PR for a given test file."""
    resolved = git_ref(base_ref)
    res = run(["git", "diff", f"{resolved}...HEAD", "--", file_path], cwd=repo_dir)
    titles = set()
    if res.returncode != 0:
        return titles
    for line in res.stdout.splitlines():
        # Match added it/test blocks with string literals
        m = re.match(r'^\+.*(?:it|test)\s*\(\s*["\']([^"\']+)["\']\s*,', line)
        if m:
            titles.add(m.group(1))
    return titles


def run_jest(cwd, extra_args):
    cmd = ["npx", "jest", "--json"]
    cmd.extend(extra_args)
    res = run(cmd, cwd=cwd)
    try:
        data = json.loads(res.stdout)
    except json.JSONDecodeError:
        print(res.stdout)
        print(res.stderr, file=sys.stderr)
        sys.exit(1)
    return data


def collect_results(results):
    """Yield (file, ancestor_titles, title, status) for each assertion."""
    for suite in results.get("testResults", []):
        file_name = suite.get("name", "")
        for assertion in suite.get("assertionResults", []):
            yield (
                file_name,
                tuple(assertion.get("ancestorTitles", [])),
                assertion.get("title", ""),
                assertion.get("status", ""),
            )


def is_new_file(repo_dir, base_ref, file_path):
    resolved = git_ref(base_ref)
    res = run(["git", "diff", "--name-status", f"{resolved}...HEAD", "--", file_path], cwd=repo_dir)
    for line in res.stdout.splitlines():
        if line.startswith("A\t") or line.startswith("A "):
            return True
    return False


def main():
    repo_dir = os.environ.get("GITHUB_WORKSPACE", os.getcwd())
    base_ref = determine_base_ref()
    resolved = git_ref(base_ref)

    if not is_sha(base_ref):
        run(["git", "fetch", "origin", base_ref], cwd=repo_dir)

    changed = changed_files(repo_dir, base_ref)
    if not changed:
        print("no mobile changes detected; nothing to test")
        sys.exit(0)

    test_files = [f for f in changed if re.search(r'\.(test|spec)\.(ts|tsx|js|jsx)$', f)]
    source_files = [f for f in changed if f.startswith("mobile/") and f not in test_files]

    mobile_dir = os.path.join(repo_dir, "mobile")

    # Install dependencies first.
    install = run(["npm", "install", "--legacy-peer-deps"], cwd=mobile_dir)
    if install.returncode != 0:
        print(install.stderr, file=sys.stderr)
        sys.exit(1)

    all_results = {"testResults": []}

    if test_files:
        # Run changed test files directly.
        paths = [os.path.join(repo_dir, f) for f in test_files]
        data = run_jest(mobile_dir, ["--runTestsByPath"] + paths)
        all_results["testResults"].extend(data.get("testResults", []))

    if source_files:
        # Run tests related to changed source files.
        paths = [os.path.join(repo_dir, f) for f in source_files]
        data = run_jest(mobile_dir, ["--findRelatedTests"] + paths)
        all_results["testResults"].extend(data.get("testResults", []))

    if not all_results["testResults"]:
        print("no Jest tests were run")
        sys.exit(0)

    # Identify newly added test titles.
    new_tests = {}
    for tf in test_files:
        full_path = os.path.join(repo_dir, tf)
        if not os.path.exists(full_path):
            continue
        if is_new_file(repo_dir, base_ref, tf):
            # Entire file is new: all tests in it are new.
            new_tests[full_path] = None  # marker: all tests in this file are new
        else:
            new_tests[full_path] = new_test_blocks_in_diff(repo_dir, base_ref, tf)

    new_failed = []
    new_passed = []
    other_failed = []

    for file_name, ancestors, title, status in collect_results(all_results):
        is_new = False
        if file_name in new_tests:
            if new_tests[file_name] is None:
                is_new = True
            elif title in new_tests[file_name]:
                is_new = True

        if is_new:
            if status == "passed":
                new_passed.append((file_name, title))
            elif status == "failed":
                new_failed.append((file_name, title))
        else:
            if status == "failed":
                other_failed.append((file_name, title))

    if new_tests:
        # Discover PR: new regression tests must fail.
        if new_passed:
            print("error: the following newly added tests passed, but should fail:", file=sys.stderr)
            for f, t in sorted(new_passed):
                print(f"  - {t} ({f})", file=sys.stderr)
            sys.exit(1)
        if not new_failed:
            print("error: no newly added test failed; regression tests should demonstrate a bug", file=sys.stderr)
            sys.exit(1)
        print(f"ok: {len(new_failed)} newly added regression test(s) failed as expected")
        sys.exit(0)
    else:
        # Fix PR: all executed tests must pass.
        if other_failed:
            print("error: the following tests failed:", file=sys.stderr)
            for f, t in sorted(other_failed):
                print(f"  - {t} ({f})", file=sys.stderr)
            sys.exit(1)
        print("ok: all executed tests passed")
        sys.exit(0)


if __name__ == "__main__":
    main()
