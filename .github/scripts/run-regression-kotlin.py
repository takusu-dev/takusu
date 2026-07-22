#!/usr/bin/env python3
"""Differential regression test runner for Kotlin/Android unit tests.

Runs the `test-android-unit` script and inspects the JUnit XML output.
For discover PRs, newly added @Test methods must fail.
For fix PRs without new tests, all executed tests must pass.
"""

import os
import re
import subprocess
import sys
import xml.etree.ElementTree as ET


def run(cmd, cwd=None, env=None):
    merged_env = {**os.environ, **(env or {})}
    print(f"+ {' '.join(cmd)} (cwd={cwd or '.'})", flush=True)
    return subprocess.run(cmd, cwd=cwd, env=merged_env, capture_output=True, text=True)


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


def class_name_from_file(repo_dir, file_path):
    """Read the class declaration from a Kotlin test file at HEAD."""
    full_path = os.path.join(repo_dir, file_path) if not os.path.isabs(file_path) else file_path
    if not os.path.exists(full_path):
        return None
    with open(full_path, "r", encoding="utf-8") as f:
        for line in f:
            m = re.search(r'\bclass\s+(\w+)', line)
            if m:
                return m.group(1)
    return None


def new_test_methods(repo_dir, base_ref):
    """Return set of (class_name, method_name) for @Test methods added in the PR."""
    resolved = git_ref(base_ref)
    res = run(["git", "diff", f"{resolved}...HEAD", "--", "mobile/modules"], cwd=repo_dir)
    if res.returncode != 0:
        print(res.stderr, file=sys.stderr)
        sys.exit(1)

    new_tests = set()
    lines = res.stdout.splitlines()
    current_file = None
    current_class = None
    idx = 0

    def find_class():
        nonlocal current_class
        if current_class is None and current_file:
            current_class = class_name_from_file(repo_dir, current_file)
        return current_class

    while idx < len(lines):
        line = lines[idx]

        # New file in diff: reset state and record the path.
        if line.startswith("diff --git"):
            current_file = None
            current_class = None
            idx += 1
            continue

        if line.startswith("+++ b/"):
            current_file = line[6:].strip()
            current_class = None
            idx += 1
            continue

        if not line.startswith("+"):
            idx += 1
            continue

        body = line[1:]

        # Class declaration in added lines (new file or new class)
        cm = re.search(r'\bclass\s+(\w+)', body)
        if cm:
            current_class = cm.group(1)
            idx += 1
            continue

        # @Test annotation, possibly with parameters or trailing whitespace.
        if re.match(r'\s*@Test\b', body):
            # Look ahead through subsequent added lines for the matching fun declaration.
            j = idx + 1
            while j < len(lines):
                nxt = lines[j]
                # Stop at diff/file boundaries or context/deletion lines.
                if nxt.startswith("diff --git") or nxt.startswith("---") or nxt.startswith("+++"):
                    break
                if nxt.startswith("+"):
                    nxt_body = nxt[1:]
                    # Skip blank, comment, or additional annotation lines.
                    if re.match(r'^\s*$', nxt_body) or re.match(r'^\s*//', nxt_body) or re.match(r'^\s*@[A-Z]', nxt_body):
                        j += 1
                        continue
                    fm = re.match(r'\s*(?:[a-z]+\s+)*fun\s+(\w+)\s*\(', nxt_body)
                    if fm:
                        cls = find_class()
                        if cls:
                            new_tests.add((cls, fm.group(1)))
                    break
                # context or deletion line: stop looking
                break
            idx = j
            continue

        idx += 1

    return new_tests


def find_junit_xml(repo_dir):
    """Search likely Gradle test output directories for JUnit XML files."""
    candidates = [
        os.path.join(repo_dir, "mobile", "modules", "takusu-widget", "android", "build", "test-results"),
        os.path.join(repo_dir, "mobile", "android", "modules", "takusu-widget", "build", "test-results"),
        os.path.join(repo_dir, "mobile", "android", "takusu-widget", "build", "test-results"),
    ]
    for base in candidates:
        if os.path.isdir(base):
            for root, _, files in os.walk(base):
                for f in files:
                    if f.startswith("TEST-") and f.endswith(".xml"):
                        yield os.path.join(root, f)


def parse_junit_xml(path):
    """Yield (class_name, method_name, status)."""
    try:
        tree = ET.parse(path)
    except ET.ParseError as e:
        print(f"warning: failed to parse {path}: {e}", file=sys.stderr)
        return
    root = tree.getroot()
    for testcase in root.findall("testcase"):
        cls = testcase.get("classname", "").split(".")[-1]
        method = testcase.get("name", "")
        failure = testcase.find("failure") is not None or testcase.find("error") is not None
        skipped = testcase.find("skipped") is not None
        status = "failed" if failure else ("skipped" if skipped else "passed")
        yield (cls, method, status)


def main():
    repo_dir = os.environ.get("GITHUB_WORKSPACE", os.getcwd())
    base_ref = determine_base_ref()

    if not is_sha(base_ref):
        run(["git", "fetch", "origin", base_ref], cwd=repo_dir)

    new_tests = new_test_methods(repo_dir, base_ref)
    print(f"new Kotlin test methods: {len(new_tests)}", flush=True)

    # Run the standard unit test script. It does prebuild and runs all widget unit tests.
    res = run(["test-android-unit"], cwd=repo_dir)
    if res.returncode != 0:
        # The script may fail because tests fail, which is expected for discover PRs.
        # Prebuild failure messages are printed to stderr; ignore test failures for now.
        print("test-android-unit exited with non-zero; parsing test output", flush=True)

    android_dir = os.path.join(repo_dir, "mobile", "android")
    if not os.path.isdir(android_dir):
        print(f"error: {android_dir} does not exist; prebuild may have failed", file=sys.stderr)
        print(res.stdout)
        print(res.stderr, file=sys.stderr)
        sys.exit(1)

    results = {}
    for xml_path in find_junit_xml(repo_dir):
        for cls, method, status in parse_junit_xml(xml_path):
            results[(cls, method)] = status

    if not results:
        print("error: no JUnit XML results found", file=sys.stderr)
        print(res.stdout)
        print(res.stderr, file=sys.stderr)
        sys.exit(1)

    if new_tests:
        # Discover PR: new regression tests must fail.
        new_failed = []
        new_passed = []
        missing = []
        for cls, method in new_tests:
            status = results.get((cls, method))
            if status == "failed":
                new_failed.append((cls, method))
            elif status == "passed":
                new_passed.append((cls, method))
            else:
                missing.append((cls, method))

        if missing:
            print(f"warning: {len(missing)} newly added tests were not found in results", flush=True)

        if new_passed:
            print("error: the following newly added Kotlin tests passed, but should fail:", file=sys.stderr)
            for cls, method in sorted(new_passed):
                print(f"  - {cls}.{method}", file=sys.stderr)
            sys.exit(1)

        if not new_failed:
            print("error: no newly added Kotlin test failed; regression tests should demonstrate a bug", file=sys.stderr)
            sys.exit(1)

        print(f"ok: {len(new_failed)} newly added Kotlin regression test(s) failed as expected")
        sys.exit(0)
    else:
        # Fix PR: all executed tests should pass.
        failures = [(k, v) for k, v in results.items() if v == "failed"]
        if failures:
            print("error: the following Kotlin tests failed:", file=sys.stderr)
            for (cls, method), _ in sorted(failures):
                print(f"  - {cls}.{method}", file=sys.stderr)
            sys.exit(1)
        print("ok: all executed Kotlin tests passed")
        sys.exit(0)


if __name__ == "__main__":
    main()
