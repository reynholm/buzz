#!/usr/bin/env python3
"""Run the hosted ACP retirement gate, refusing absent or zero-matched tests."""
import os
import re
import subprocess
import sys

WINDOWS = sys.platform == "win32"
MACOS = sys.platform == "darwin"
if not (WINDOWS or MACOS):
    raise SystemExit("The hosted retirement gate requires native Windows or macOS")

for name in (
    "BUZZ_ACP_ALLOWED_RESPOND_TO", "BUZZ_ACP_LAZY_POOL", "BUZZ_ACP_IDLE_POOL_SLEEP",
    "BUZZ_ACP_SESSION_POLICY", "BUZZ_ACP_SESSION_STORE_DIR", "BUZZ_ACP_NO_SESSION_RESUME",
    "BUZZ_ACP_LAUNCH_PREFIX",
):
    os.environ.pop(name, None)

base = ["cargo", "test", "--locked", "-p", "buzz-acp", "--lib"]
# Discovery and execution use the same filters, then compare exact counts.
gates = [("checked_termination", 6 if WINDOWS else 8), ("portable_retirement", 4)]
if MACOS:
    gates.append(("checked_retirement", None))
for name, expected in gates:
    discovered = subprocess.run(base + [name, "--", "--list"], check=True,
                                capture_output=True, text=True).stdout
    tests = [line.removesuffix(": test") for line in discovered.splitlines()
             if line.endswith(": test")]
    if not tests or (expected is not None and len(tests) != expected):
        raise SystemExit(f"{name}: expected {expected or 'nonzero'} tests, found {tests}")
    if name == "checked_retirement" and not any(
        test.endswith("checked_retirement_executor_waits_for_verified_kill_and_excludes_refill")
        for test in tests
    ):
        raise SystemExit("Missing actual worker-retirement executor regression")
    print(f"Native {sys.platform}: {name}: {len(tests)} tests\n" + "\n".join(tests), flush=True)
    result = subprocess.run(base + [name, "--", "--test-threads=1"],
                            capture_output=True, text=True)
    print(result.stdout, end="", flush=True)
    print(result.stderr, end="", file=sys.stderr, flush=True)
    result.check_returncode()
    summary = re.search(r"test result: ok\. (\d+) passed; 0 failed; 0 ignored;", result.stdout)
    if summary is None or int(summary[1]) != len(tests):
        raise SystemExit(f"{name}: executed count differs from discovered {len(tests)} tests")
