import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

# Use the local identity wrapper when installed; CI can use its native Git.
IDENTITY_WRAPPER = Path.home() / '.local/bin/git'
GIT = str(IDENTITY_WRAPPER) if IDENTITY_WRAPPER.is_file() else 'git'
MODULE = Path(__file__).resolve().parents[1] / 'sync.py'
spec = importlib.util.spec_from_file_location('fork_sync', MODULE)
sync = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sync)


class RegistryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.name', 'Fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        (self.repo / 'caller.py').write_text('def run():\n    guard()\n')
        self.git('add', '.')
        self.git('-c', 'commit.gpgsign=false', 'commit', '-qm', 'base')
        self.base = self.git('rev-parse', 'HEAD').strip()
        self.manifest = {'base_tag': 'fixture', 'base_sha': self.base, 'patches': []}
        self.markdown()

    def git(self, *args):
        return subprocess.check_output([GIT, '-C', str(self.repo), *args], text=True)

    def markdown(self):
        (self.repo / 'FORK_PATCHES.md').write_text(sync.render_registry(self.manifest))

    def entry(self):
        return {'path': 'caller.py', 'responsibility': 'Guard execution', 'is_new': False,
                'required_symbols': ['def run():'], 'invocation_seams': [
                    {'caller_path': 'caller.py', 'callee_symbol': 'guard', 'invocation': 'guard()',
                     'behavior_test': 'test_execution_is_guarded', 'verification_command': 'python3 -m unittest'}],
                'verification_commands': ['python3 -m unittest']}

    def test_registry_rejects_unlisted_modified_upstream_path(self):
        (self.repo / 'caller.py').write_text('changed\n')
        self.assertIn('unlisted path: caller.py', sync.validate_patches(self.repo, self.manifest))

    def test_registry_rejects_missing_seam(self):
        self.manifest['patches'] = [self.entry()]
        self.markdown()
        self.assertEqual(sync.validate_patches(self.repo, self.manifest), [])
        (self.repo / 'caller.py').write_text('def run():\n    pass\n')
        self.assertIn('missing invocation', '\n'.join(sync.validate_patches(self.repo, self.manifest)))

    def test_registry_markdown_matches_json(self):
        (self.repo / 'FORK_PATCHES.md').write_text('drift')
        self.assertIn('Markdown registry differs', sync.validate_patches(self.repo, self.manifest))

    def test_cli_rejects_unlisted_path(self):
        directory = self.repo / 'scripts/fork'
        directory.mkdir(parents=True)
        (directory / 'patches.json').write_text(json.dumps(self.manifest))
        (self.repo / 'caller.py').write_text('changed\n')
        result = subprocess.run(['python3', str(MODULE), '--repo', str(self.repo)],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('unlisted path: caller.py', result.stdout)

    def test_registered_binary_asset_needs_no_text_symbols(self):
        entry = self.entry()
        entry.update(path='asset.bin', is_new=True, required_symbols=[], invocation_seams=[])
        self.manifest['patches'] = [entry]
        (self.repo / 'asset.bin').write_bytes(b'\xff\x00')
        self.git('add', 'asset.bin')
        self.markdown()
        try:
            errors = sync.validate_patches(self.repo, self.manifest)
        except UnicodeDecodeError:
            self.fail('Registered assets without text symbols must validate as paths')
        self.assertEqual(errors, [])

    def test_registered_upstream_deletion_is_verified(self):
        entry = self.entry()
        entry.update(deleted=True, required_symbols=[], invocation_seams=[])
        self.manifest['patches'] = [entry]
        self.git('rm', 'caller.py')
        self.markdown()
        self.assertEqual(sync.validate_patches(self.repo, self.manifest), [])
        (self.repo / 'caller.py').write_text('restored unexpectedly\n')
        self.assertIn('registered deletion still exists: caller.py',
                      sync.validate_patches(self.repo, self.manifest))

    def test_deletion_cannot_mask_a_missing_new_module_or_required_guard(self):
        for overrides in [dict(path='new.py', is_new=True), dict()]:
            entry = self.entry()
            entry.update(deleted=True, **overrides)
            self.manifest['patches'] = [entry]
            self.markdown()
            self.assertIn('invalid registered deletion: ' + entry['path'],
                          sync.validate_patches(self.repo, self.manifest))

    def test_new_module_cannot_disappear(self):
        entry = self.entry()
        entry.update(path='new.py', is_new=True, required_symbols=[], invocation_seams=[])
        self.manifest['patches'] = [entry]
        self.markdown()
        self.assertIn('missing path: new.py', sync.validate_patches(self.repo, self.manifest))


if __name__ == '__main__':
    unittest.main()
