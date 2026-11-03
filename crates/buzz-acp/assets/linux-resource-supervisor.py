"""Keep verified OOM evidence and reap one ACP worker's owned cgroup."""
import json
import os
import pathlib
import subprocess
import sys
import time

parent_umask = os.umask(0o077)
state_file, expected_group, high, maximum, expected_parent, *worker = sys.argv[1:]
state_file = pathlib.Path(state_file)
parent = int(expected_parent)


def record(state):
    temporary = state_file.with_suffix(".next")
    temporary.write_text(json.dumps({"state": state}))
    temporary.replace(state_file)


def bounded_read(path):
    with path.open() as stream:
        data = stream.read(4097)
    if len(data) > 4096:
        raise ValueError("oversized cgroup state")
    return data.strip()


def kill_group():
    (group / "cgroup.kill").write_text("1")


try:
    membership = bounded_read(pathlib.Path("/proc/self/cgroup"))
    relative = membership.removeprefix("0::")
    if membership != "0::" + expected_group:
        raise ValueError("incorrect cgroup membership")
    group = pathlib.Path("/sys/fs/cgroup") / relative.lstrip("/")
    for filename, expected in [("memory.high", high), ("memory.max", maximum),
                               ("memory.swap.max", "0")]:
        if bounded_read(group / filename) != expected:
            raise ValueError("incorrect cgroup limit")
    if not os.access(group / "cgroup.kill", os.W_OK):
        raise ValueError("cgroup retirement unavailable")
    # Zero is required for a newly created, uniquely named worker scope.
    def oom_kills():
        counters = dict(line.split() for line in bounded_read(group / "memory.events").splitlines())
        return int(counters["oom_kill"])
    if oom_kills() != 0:
        raise ValueError("scope already experienced OOM")
    # The harness may have exited before Python finished initializing.
    if os.getppid() != parent:
        record("parent_exited")
        kill_group()
        sys.exit(1)
    record("ready")
    child = subprocess.Popen(worker, umask=parent_umask)
    while True:
        if oom_kills() > 0:
            # Preserve the cause before cgroup.kill removes this supervisor too.
            record("oom")
            kill_group()
            sys.exit(1)
        if os.getppid() != parent:
            record("parent_exited")
            kill_group()
            sys.exit(1)
        status = child.poll()
        if status is not None:
            # The kernel can kill the adapter between the loop's counter read
            # and poll(). Capture that cause before deleting the cgroup.
            record("oom" if oom_kills() > 0 else "exited")
            # Retire daemonized descendants even on a normal adapter exit.
            kill_group()
            sys.exit(status if status >= 0 else 128 - status)
        time.sleep(0.05)
except Exception:
    # Do not expose argv, environment or provider credentials in diagnostics.
    try:
        record("guard_failed")
        if "group" in globals():
            kill_group()
    finally:
        sys.exit(1)
