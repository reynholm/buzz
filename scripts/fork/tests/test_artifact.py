"""Behavioral artifact guards; native startup is replaced only at the subprocess boundary."""
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import struct
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

MODULE = Path(__file__).resolve().parents[1] / "verify_artifact.py"
spec = importlib.util.spec_from_file_location("verify_artifact", MODULE)
artifact = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = artifact
spec.loader.exec_module(artifact)
SHA = "a" * 40
BASE = {"version": "0.5.26", "identifier": "xyz.block.buzz.app", "base_tag": "desktop-v0.5.26", "fork_revision": "1"}
SIDECARS = ["buzz-acp", "buzz-agent", "buzz-backend-kubernetes", "buzz-dev-mcp", "git-credential-nostr", "buzz"]

class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.app = Path(self.temp.name) / "Buzz.app"
        self.binaries = self.app / "Contents/MacOS"
        self.binaries.mkdir(parents=True)
        (self.app / "Contents/Resources").mkdir()
        self.plist = {"CFBundleExecutable": "buzz-desktop", "CFBundleIdentifier": BASE["identifier"], "CFBundleShortVersionString": BASE["version"]}
        self.write_plist()
        for name in ["buzz-desktop", *SIDECARS]:
            binary = self.binaries / name
            binary.write_bytes(struct.pack("<IIIIIIII", 0xfeedfacf, 0x100000c, 0, 2, 0, 0, 0, 0) + b"fixture")
            binary.chmod(0o755)
        self.probe = {"identity": {"fork_revision": "1", "commit_sha": SHA, "base_tag": BASE["base_tag"]}, "updater_enabled": False, "demo_slug": None, "config": {"version": BASE["version"], "identifier": BASE["identifier"], "plugins": {"updater": {"endpoints": []}}}}
        self.subprocess = patch.object(artifact, "probe_artifact", lambda _: json.loads(json.dumps(self.probe)), create=True)
        self.subprocess.start()
        self.addCleanup(self.subprocess.stop)
    def write_plist(self):
        (self.app / "Contents/Info.plist").write_bytes(plistlib.dumps(self.plist))
    def verify(self):
        return artifact.verify_artifact(self.app, SHA, BASE)
    def test_identity_matches_candidate(self):
        self.assertEqual(self.verify().commit_sha, SHA)
        for field, wrong in [("commit_sha", "b" * 40), ("base_tag", "desktop-v0.5.25"), ("fork_revision", "2")]:
            with self.subTest(field=field):
                original = self.probe["identity"][field]
                self.probe["identity"][field] = wrong
                with self.assertRaises(ValueError): self.verify()
                self.probe["identity"][field] = original
    def test_placeholder_or_wrong_arch_is_rejected(self):
        binary = self.binaries / "buzz-acp"
        original = binary.read_bytes()
        for data, mode in [(b"", 0o755), (b"#!/bin/sh\nexit 0", 0o755), (original, 0o644), (struct.pack("<IIIIIIII", 0xfeedfacf, 0x1000007, 0, 2, 0, 0, 0, 0), 0o755)]:
            with self.subTest(mode=mode, size=len(data)):
                binary.write_bytes(data); binary.chmod(mode)
                with self.assertRaises(ValueError): self.verify()
        binary.write_bytes(original); binary.chmod(0o755)
        self.verify()
    def test_bundled_updater_is_rejected(self):
        self.probe["config"]["plugins"]["updater"]["endpoints"] = ["https://updater.invalid"]
        with self.assertRaises(ValueError): self.verify()
        self.probe["config"]["plugins"]["updater"]["endpoints"] = []
        self.probe["updater_enabled"] = True
        with self.assertRaises(ValueError): self.verify()
        self.probe["updater_enabled"] = False
        (self.app / "Contents/Resources/hidden.json").write_text(json.dumps({"plugins": {"updater": {"endpoints": ["https://updater.invalid"]}}}))
        with self.assertRaises(ValueError): self.verify()
    def test_upstream_version_identifier_and_demo_namespace_are_preserved(self):
        for field in ["CFBundleIdentifier", "CFBundleShortVersionString"]:
            original = self.plist[field]; self.plist[field] = "changed"; self.write_plist()
            with self.assertRaises(ValueError): self.verify()
            self.plist[field] = original; self.write_plist()
        self.probe["demo_slug"] = "accidental-demo"
        with self.assertRaises(ValueError): self.verify()
    def test_probe_uses_early_readonly_command(self):
        self.subprocess.stop()
        with patch.object(artifact, "subprocess", create=True) as run:
            run.run.return_value.stdout = json.dumps(self.probe)
            self.assertEqual(artifact.probe_artifact(self.binaries / "buzz-desktop"), self.probe)
            self.assertEqual(run.run.call_args.args[0], [str(self.binaries / "buzz-desktop"), "--fork-artifact-probe"])
            self.assertEqual(run.run.call_args.kwargs["timeout"], 30)

class BaselineTests(unittest.TestCase):
    def test_exact_successful_baseline_is_required(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'baseline.json'
            registry = {'base_sha': SHA}
            receipts = {'macos/Buzz.app/Contents/MacOS/' + name: {'size': 100, 'sha256': 'a' * 64} for name in ['buzz-desktop', *SIDECARS]}
            receipts['dmg/Buzz_0.5.26_aarch64.dmg'] = {'size': 100, 'sha256': 'a' * 64}
            evidence = {'upstream_sha': SHA, 'result': 'success', 'gate_commands': [['just', 'ci']], 'artifact_manifest': receipts}
            path.write_text(json.dumps(evidence))
            self.assertEqual(artifact.load_baseline(path, registry), evidence)
            for key, value in [('upstream_sha', 'b' * 40), ('result', 'failure'), ('gate_commands', []), ('artifact_manifest', {})]:
                wrong = {**evidence, key: value}; path.write_text(json.dumps(wrong))
                with self.subTest(key=key), self.assertRaises(ValueError):
                    artifact.load_baseline(path, registry)
            receipts['macos/Buzz.app/Contents/MacOS/buzz']['size'] = 0
            path.write_text(json.dumps(evidence))
            with self.assertRaises(ValueError): artifact.load_baseline(path, registry)

class BuildScriptTests(unittest.TestCase):
    def test_actual_build_command_fails_closed_before_artifacts(self):
        # Run the production producer contract in a disposable local source checkout.
        source = MODULE.parents[2]
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            (repo / 'scripts/fork').mkdir(parents=True)
            for name in ['build-candidate.sh', 'verify_artifact.py', 'patches.json']:
                shutil.copy2(source / 'scripts/fork' / name, repo / 'scripts/fork' / name)
            def git(*args):
                return subprocess.check_output(['git', *args], cwd=repo, text=True).strip()
            git('init', '-q'); git('add', 'scripts')
            git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture source')
            sha = git('rev-parse', 'HEAD')
            baseline = repo / 'invalid-baseline.json'
            baseline.write_text(json.dumps({'result': 'failure'}))
            # Keep the supplied receipt outside tracked candidate source.
            receipt = Path(directory).parent / (repo.name + '-baseline.json')
            shutil.move(baseline, receipt)
            try:
                for candidate, diagnostic in [(sha, 'exact successful upstream baseline'), ('a' * 40, 'Candidate SHA differs')]:
                    result = subprocess.run(['bash', str(repo / 'scripts/fork/build-candidate.sh'), '--candidate-sha', candidate, '--baseline-report', str(receipt)], cwd=repo, text=True, capture_output=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(diagnostic, result.stderr)
                    self.assertFalse((repo / 'artifacts').exists())
            finally:
                receipt.unlink()

    def test_dirty_source_is_rejected_before_build_side_effects(self):
        # A valid baseline gets past preflight; a fake compiler records any build.
        source = MODULE.parents[2]
        for dirty_kind in ['unstaged', 'staged', 'untracked']:
            with self.subTest(dirty_kind=dirty_kind), tempfile.TemporaryDirectory() as directory:
                workspace = Path(directory)
                repo = workspace / 'repo'
                (repo / 'scripts/fork').mkdir(parents=True)
                for name in ['build-candidate.sh', 'verify_artifact.py', 'patches.json']:
                    shutil.copy2(source / 'scripts/fork' / name, repo / 'scripts/fork' / name)
                tracked = repo / 'tracked.txt'
                tracked.write_text('clean candidate\n')
                def git(*args):
                    return subprocess.check_output(['git', *args], cwd=repo, text=True).strip()
                git('init', '-q'); git('add', '.')
                git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'candidate fixture')
                sha = git('rev-parse', 'HEAD')
                registry = json.loads((repo / 'scripts/fork/patches.json').read_text())
                receipts = {'macos/Buzz.app/Contents/MacOS/' + name: {'size': 100, 'sha256': 'a' * 64} for name in ['buzz-desktop', *SIDECARS]}
                receipts['dmg/Buzz_0.5.26_aarch64.dmg'] = {'size': 100, 'sha256': 'a' * 64}
                baseline = workspace / 'baseline.json'
                baseline.write_text(json.dumps({'upstream_sha': registry['base_sha'], 'result': 'success', 'gate_commands': [['fixture']], 'artifact_manifest': receipts}))
                marker = workspace / 'compiler-invoked'
                tools = workspace / 'tools'; tools.mkdir()
                cargo = tools / 'cargo'
                cargo.write_text('#!/bin/sh\ntouch "$M3_TEST_COMPILER_MARKER"\nexit 23\n')
                cargo.chmod(0o755)
                env = {**os.environ, 'PATH': str(tools) + os.pathsep + os.environ['PATH'], 'M3_TEST_COMPILER_MARKER': str(marker)}
                if dirty_kind == 'untracked':
                    (repo / 'extra-source.txt').write_text('unexpected source\n')
                else:
                    tracked.write_text('changed after candidate capture\n')
                    if dirty_kind == 'staged': git('add', 'tracked.txt')
                result = subprocess.run(['bash', str(repo / 'scripts/fork/build-candidate.sh'), '--candidate-sha', sha, '--baseline-report', str(baseline)], cwd=repo, env=env, text=True, capture_output=True, timeout=30)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((repo / 'artifacts').exists(), f'{dirty_kind} source entered build: {result.stdout} {result.stderr}')
                self.assertFalse(marker.exists(), f'{dirty_kind} source invoked compiler')
