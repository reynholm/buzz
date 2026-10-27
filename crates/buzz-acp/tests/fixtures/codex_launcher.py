#!/usr/bin/env python3
"""Trace the launcher boundary, then exec the actual adapter or external CLI."""
import json
import os
from pathlib import Path
import sys

environment = os.environ.copy()
if Path(sys.argv[0]).name == "codex-acp":
    target = environment["BUZZ_TEST_LAUNCHER_ADAPTER"]
    inherited = environment.get("CODEX_PATH")
    selected = inherited or environment["BUZZ_TEST_LAUNCHER_FALLBACK"]
    # Negative acceptance control for an operator wrapper that loses the proxy.
    if environment.get("BUZZ_TEST_LAUNCHER_OVERWRITE") == "1":
        selected = environment["BUZZ_TEST_LAUNCHER_FALLBACK"]
    environment["CODEX_PATH"] = selected
    kind = "launcher"
else:
    target = environment["BUZZ_TEST_LAUNCHER_REAL_CLI"]
    inherited = None
    selected = None
    kind = "external"

record = {
    "kind": kind,
    "pid": os.getpid(),
    "args": sys.argv[1:],
    "inherited": inherited,
    "selected": selected,
    "target": target,
}
fd = os.open(environment["BUZZ_TEST_LAUNCHER_TRACE"], os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
try:
    os.write(fd, (json.dumps(record) + "\n").encode())
finally:
    os.close(fd)
os.execve(target, [target, *sys.argv[1:]], environment)
