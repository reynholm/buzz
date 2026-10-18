#!/usr/bin/env python3
"""Run the hosted ACP retirement gate, refusing absent or zero-matched tests."""
import datetime
import json
import os
import platform
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

def source_receipt():
    """Report the actual checkout, including synthetic Actions merge commits."""
    return {
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "tree": subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], text=True).strip(),
        "status": subprocess.check_output(["git", "status", "--porcelain"], text=True),
    }


before = source_receipt()
print(json.dumps({"phase": "before", "source": before,
    "at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "os": platform.platform(), "python": sys.version,
    "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
    "cargo": subprocess.check_output(["cargo", "--version"], text=True).strip()}), flush=True)

base = ["cargo", "test", "--locked", "-p", "buzz-acp", "--lib"]
# Discovery and execution use the same filters, then compare exact counts.
gates = [("checked_termination", 6 if WINDOWS else 8), ("portable_retirement", 4)]
if MACOS:
    gates.append(("checked_retirement", None))
try:
    for name, expected in gates:
        discovery = subprocess.run(base + [name, "--", "--list"], check=False,
                                   capture_output=True, encoding="utf-8", errors="replace")
        print(discovery.stderr, end="", file=sys.stderr, flush=True)
        if discovery.returncode:
            print(discovery.stdout, end="", flush=True)
        discovery.check_returncode()
        discovered = discovery.stdout
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
                                capture_output=True, encoding="utf-8", errors="replace")
        print(result.stdout, end="", flush=True)
        print(result.stderr, end="", file=sys.stderr, flush=True)
        result.check_returncode()
        summary = re.search(r"test result: ok\. (\d+) passed; 0 failed; 0 ignored;", result.stdout)
        if summary is None or int(summary[1]) != len(tests):
            raise SystemExit(f"{name}: executed count differs from discovered {len(tests)} tests")
finally:
    after = source_receipt()
    print(json.dumps({"phase": "after", "source": after,
        "at": datetime.datetime.now(datetime.timezone.utc).isoformat()}), flush=True)
    if after != before:
        raise SystemExit("Tested source changed during the retirement gate")
