"""Execute the production supervisor against a deterministic kernel-exit race."""
import io
import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch

SOURCE = (pathlib.Path(__file__).parents[1] / "assets/linux-resource-supervisor.py").read_text()


class SupervisorExitRace(unittest.TestCase):
    def test_oom_between_counter_read_and_child_poll_remains_terminal(self):
        receipts = []
        counters = {"oom_kill": 0}
        pending = {}

        class FakePath:
            def __init__(self, value):
                self.value = str(value)

            def __truediv__(self, suffix):
                return FakePath(self.value.rstrip("/") + "/" + suffix)

            def with_suffix(self, suffix):
                return FakePath("/state" + suffix)

            def open(self):
                if self.value == "/proc/self/cgroup":
                    return io.StringIO("0::/owned.scope")
                if self.value.endswith("memory.events"):
                    return io.StringIO("oom_kill " + str(counters["oom_kill"]))
                if self.value.endswith(("memory.high", "memory.max")):
                    return io.StringIO("128")
                if self.value.endswith("memory.swap.max"):
                    return io.StringIO("0")
                raise AssertionError("unexpected read: " + self.value)

            def write_text(self, data):
                pending[self.value] = data

            def replace(self, target):
                receipts.append(json.loads(pending[self.value])["state"])

        class Child:
            def poll(self):
                counters["oom_kill"] = 1
                return -9

        with patch("pathlib.Path", FakePath), patch("os.getppid", return_value=42), \
                patch("os.access", return_value=True), patch("subprocess.Popen", return_value=Child()), \
                patch("sys.argv", ["supervisor", "/state.json", "/owned.scope", "128", "128", "42", "worker"]):
            with self.assertRaises(SystemExit):
                exec(compile(SOURCE, "linux-resource-supervisor.py", "exec"), {})
        self.assertEqual(counters["oom_kill"], 1)
        self.assertEqual(receipts, ["ready", "oom"])

    def test_owner_exit_before_supervisor_initialization_refuses_payload(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            state = root / "state.json"
            group = root / "cgroup"
            group.mkdir()
            for name, value in {"memory.high": "128", "memory.max": "128",
                                "memory.swap.max": "0", "memory.events": "oom_kill 0",
                                "cgroup.kill": ""}.items():
                (group / name).write_text(value)
            real_open = pathlib.Path.open

            def open_path(path, *args, **kwargs):
                if str(path) == "/proc/self/cgroup":
                    return io.StringIO("0::/owned.scope")
                if str(path).startswith("/sys/fs/cgroup/owned.scope"):
                    path = group / path.name
                return real_open(path, *args, **kwargs)

            for adopted_parent in [1, 17]:
                with self.subTest(adopted_parent=adopted_parent), \
                        patch("pathlib.Path.open", open_path), \
                        patch("os.getppid", return_value=adopted_parent), \
                        patch("os.access", return_value=True), \
                        patch("subprocess.Popen") as payload, \
                        patch("sys.argv", ["supervisor", str(state), "/owned.scope",
                                           "128", "128", "42", "worker"]):
                    payload.return_value.poll.return_value = 0
                    with self.assertRaises(SystemExit):
                        exec(compile(SOURCE, "linux-resource-supervisor.py", "exec"), {})
                    self.assertFalse(payload.called, "payload started after its owner exited")
                    self.assertNotEqual(json.loads(state.read_text())["state"], "ready")
                    self.assertEqual((group / "cgroup.kill").read_text(), "1")


if __name__ == "__main__":
    unittest.main()
