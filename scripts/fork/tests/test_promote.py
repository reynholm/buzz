"""Synthetic acceptance and disposable real Git/asset fixtures, never owner evidence."""
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import plistlib
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest

from test_patch_registry import GIT, sync

MODULE = Path(__file__).resolve().parents[1] / 'promote.py'
sys.path.insert(0, str(MODULE.parent))
spec = importlib.util.spec_from_file_location('fork_promote', MODULE) if MODULE.exists() else None
promote = importlib.util.module_from_spec(spec) if spec else None
if spec:
    spec.loader.exec_module(promote)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def gh_fixture(root, argv):
    """A local process boundary for GH; state/assets live only in a disposable dir."""
    import shutil
    state_path = root / 'gh.json'
    state = json.loads(state_path.read_text())
    state['calls'].append(argv)
    result = ''
    if argv[0] == 'api':
        endpoint = argv[1]
        if '/pulls/' in endpoint:
            if state.get('tamper_on_pr'):
                Path(state['tamper_on_pr']).write_text('{}')
            result = json.dumps(state['pr'])
        elif endpoint.endswith('/releases?per_page=100'):
            items = [state['release']] if state.get('release') else []
            if state.get('paginate_needed') and '--paginate' not in argv:
                items = [{'tag_name': 'other-' + str(i)} for i in range(100)]
            result = json.dumps([items] if '--slurp' in argv else items)
        elif '/releases/tags/' in endpoint:
            result = json.dumps(state['release'])
        else:
            raise AssertionError(argv)
    elif argv[:2] == ['release', 'create']:
        assert '--draft' in argv and '--verify-tag' in argv
        assert state['release'] is None, 'release already exists'
        state['release'] = {'tag_name': argv[2], 'body': Path(argv[argv.index('--notes-file')+1]).read_text(),
                            'target_commitish': argv[argv.index('--target')+1], 'draft': True,
                            'assets': [], 'html_url': 'https://github.com/reynholm/buzz/releases/tag/'+argv[2]}
        result = state['release']['html_url']
    elif argv[:2] == ['release', 'upload']:
        assert '--clobber' not in argv
        source = Path(argv[3])
        (root / 'remote-assets').mkdir(exist_ok=True)
        shutil.copyfile(source, root / 'remote-assets' / source.name)
        state['release']['assets'].append({'name': source.name})
    elif argv[:2] == ['release', 'download']:
        target = Path(argv[argv.index('--dir')+1])
        for source in (root / 'remote-assets').iterdir():
            shutil.copyfile(source, target / source.name)
    elif argv[:2] == ['release', 'edit']:
        assert '--draft=false' in argv
        state['release']['draft'] = False
    else:
        raise AssertionError(argv)
    state_path.write_text(json.dumps(state))
    return result


class PromotionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / 'repo'
        self.repo.mkdir()
        self.git('init', '-q')
        self.git('config', 'user.name', 'Synthetic fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        directory = self.repo / 'desktop/src-tauri'
        directory.mkdir(parents=True)
        self.config = {'version': '0.5.26', 'identifier': 'xyz.block.buzz.app',
                       'plugins': {'updater': {'endpoints': []}}}
        (directory / 'tauri.conf.json').write_text(json.dumps(self.config))
        self.commit('synthetic upstream')
        self.base = self.git('rev-parse', 'HEAD').strip()
        self.git('tag', 'desktop-v0.5.26')
        (self.repo / 'scripts/fork').mkdir(parents=True)
        self.registry = {'base_sha': self.base, 'base_tag': 'desktop-v0.5.26', 'patches': []}
        (self.repo / 'scripts/fork/patches.json').write_text(json.dumps(self.registry))
        self.commit('synthetic candidate')
        self.candidate = self.git('rev-parse', 'HEAD').strip()
        self.tree = self.git('rev-parse', 'HEAD^{tree}').strip()
        self.git('branch', 'fork/sync-v0.5.26')
        self.git('checkout', '-qb', 'fork/main')
        self.git('-c', 'commit.gpgsign=false', 'commit', '--allow-empty', '-qsm', 'synthetic owner merge')
        self.merged = self.git('rev-parse', 'HEAD').strip()
        self.assets = self.root / 'assets'
        self.assets.mkdir()
        app = self.root / 'Buzz.app'
        binaries = app / 'Contents/MacOS'
        binaries.mkdir(parents=True)
        (app / 'Contents/Info.plist').write_bytes(plistlib.dumps({
            'CFBundleExecutable': 'buzz-desktop', 'CFBundleIdentifier': self.config['identifier'],
            'CFBundleShortVersionString': '0.5.26'}))
        header = struct.pack('<8I', 0xfeedfacf, 0x100000c, 0, 2, 0, 0, 0, 0)
        records = {}
        for name in ('buzz-desktop', 'buzz-acp', 'buzz-agent', 'buzz-backend-kubernetes',
                     'buzz-dev-mcp', 'git-credential-nostr', 'buzz'):
            path = binaries / name
            path.write_bytes(header + b'SYNTHETIC NOT A RUNNABLE APP')
            path.chmod(0o755)
            records[name] = {'size': path.stat().st_size, 'sha256': sha(path),
                             'mode': '0o755', 'arch': 'arm64'}
        archive = self.assets / 'Buzz_0.5.26_aarch64.app.tar.gz'
        with tarfile.open(archive, 'w:gz') as stream:
            stream.add(app, arcname='Buzz.app')
        dmg = self.assets / 'Buzz_0.5.26_aarch64.dmg'
        dmg.write_bytes(b'SYNTHETIC DMG NOT INSTALLABLE')
        baseline = {'upstream_sha': self.base, 'result': 'success',
                    'gate_commands': ['synthetic full gate'], 'artifact_manifest': {
                        'macos/Buzz.app/Contents/MacOS/' + n: {'size': r['size'], 'sha256': r['sha256']}
                        for n, r in records.items()}}
        baseline['artifact_manifest']['dmg/synthetic.dmg'] = {'size': 1, 'sha256': 'a'*64}
        (self.assets / 'baseline.json').write_text(json.dumps(baseline))
        self.manifest = {'commit_sha': self.candidate, 'fork_revision': '1',
                         'base_tag': 'desktop-v0.5.26', 'arch': 'arm64', 'version': '0.5.26',
                         'identifier': self.config['identifier'], 'binaries': records,
                         'resources': {}, 'embedded_config': self.config, 'updater_enabled': False,
                         'baseline': {'upstream_sha': self.base,
                                      'report_sha256': sha(self.assets / 'baseline.json'),
                                      'artifact_manifest': baseline['artifact_manifest']},
                         'dmg': {'size': dmg.stat().st_size, 'sha256': sha(dmg)}}
        (self.assets / 'manifest.json').write_text(json.dumps(self.manifest))
        names = [archive.name, dmg.name, 'manifest.json', 'baseline.json']
        (self.assets / 'SHA256SUMS').write_text(''.join(sha(self.assets / n)+'  '+n+'\n' for n in names))
        self.receipt = {'synthetic': True, 'owner_login': 'reynholm',
                        'pr_url': 'https://github.com/reynholm/buzz/pull/123',
                        'candidate_sha': self.candidate, 'candidate_tree': self.tree,
                        'merged_sha': self.merged, 'base_tag': 'desktop-v0.5.26',
                        'upstream_sha': self.base, 'manifest_sha256': sha(self.assets / 'manifest.json'),
                        'checksums_sha256': sha(self.assets / 'SHA256SUMS'),
                        'evidence': {name: {'accepted': True, 'reference': 'synthetic fixture only'}
                                     for name in ('owner_review', 'full_tests', 'native_about',
                                                  'existing_data', 'sidecar_runtime', 'keychain_prompt',
                                                  'two_physical_devices', 'maintenance_command')}}

    def git(self, *args):
        return subprocess.check_output([GIT, '-C', str(self.repo), *args], text=True)

    def commit(self, message):
        self.git('add', '.')
        self.git('-c', 'commit.gpgsign=false', 'commit', '-qsm', message)

    def envelope(self):
        self.assertIsNotNone(promote, 'owner/tree promotion tooling is missing')
        return {**self.manifest, 'promotion': {'artifact_dir': str(self.assets), 'acceptance': self.receipt}}

    def verify(self, merged=None):
        return promote.verify_promotion(self.repo, self.candidate, merged or self.merged, self.envelope())

    def test_premerge_candidate_cannot_tag(self):
        envelope = self.envelope()
        envelope['promotion']['acceptance']['merged_sha'] = None
        with self.assertRaises(ValueError):
            promote.verify_promotion(self.repo, self.candidate, self.candidate, envelope)
        self.assertEqual(self.git('tag', '--list', 'fork-v*'), '')

    def test_changed_merge_tree_cannot_promote(self):
        self.envelope()
        (self.repo / 'changed.txt').write_text('merge changed tested tree')
        self.commit('changed merge')
        changed = self.git('rev-parse', 'HEAD').strip()
        self.receipt['merged_sha'] = changed
        with self.assertRaisesRegex(ValueError, 'tree'):
            self.verify(changed)
        self.assertEqual(self.git('tag', '--list', 'fork-v*'), '')
        self.assertNotEqual(self.git('branch', '--list', 'fork/sync-*'), '')

    def test_postmerge_promotion_keeps_tested_tree(self):
        envelope = self.envelope()
        result = self.verify()
        self.assertEqual(result.candidate_sha, self.candidate)
        self.assertEqual(result.merged_sha, self.merged)
        self.assertEqual(result.tree, self.tree)
        self.assertEqual(result.tag, 'fork-v0.5.26-1')
        self.assertEqual(sha(self.assets / 'manifest.json'), self.receipt['manifest_sha256'])
        self.assertEqual(self.git('tag', '--list', 'fork-v*'), '')  # verify is read-only

    def test_existing_tag_is_idempotent_only_for_same_commit(self):
        self.envelope()
        self.git('tag', 'fork-v0.5.26-1', self.merged)
        self.assertEqual(self.verify().merged_sha, self.merged)
        self.git('tag', '-d', 'fork-v0.5.26-1')
        self.git('tag', 'fork-v0.5.26-1', self.candidate)  # same TREE still wrong COMMIT
        with self.assertRaisesRegex(ValueError, 'tag'):
            self.verify()

    def test_missing_or_stale_acceptance_rejects_before_mutations(self):
        self.envelope()
        original = copy.deepcopy(self.receipt)
        for name in original['evidence']:
            with self.subTest(name=name):
                self.receipt = copy.deepcopy(original)
                self.receipt['evidence'][name]['accepted'] = False
                with self.assertRaises(ValueError):
                    self.verify()
        for key in ('candidate_sha', 'candidate_tree', 'upstream_sha', 'manifest_sha256', 'checksums_sha256'):
            with self.subTest(key=key):
                self.receipt = copy.deepcopy(original)
                self.receipt[key] = '0' * len(original[key])
                with self.assertRaises(ValueError):
                    self.verify()
        self.assertEqual(self.git('tag', '--list', 'fork-v*'), '')

    def test_tampered_asset_or_archive_binary_cannot_promote(self):
        self.envelope()
        dmg = self.assets / 'Buzz_0.5.26_aarch64.dmg'
        dmg.write_bytes(b'tampered')
        with self.assertRaisesRegex(ValueError, 'checksum'):
            self.verify()

    def test_rechecks_archived_binary_bytes_even_with_resealed_checksums(self):
        self.envelope()
        archive = self.assets / 'Buzz_0.5.26_aarch64.app.tar.gz'
        rewritten = self.assets / 'rewritten.tar.gz'
        with tarfile.open(archive, 'r:gz') as source, tarfile.open(rewritten, 'w:gz') as target:
            for member in source:
                content = source.extractfile(member).read() if member.isfile() else None
                if member.name == 'Buzz.app/Contents/MacOS/buzz-acp':
                    content = content[:-1] + b'X'
                target.addfile(member, io.BytesIO(content) if content is not None else None)
        rewritten.replace(archive)
        sums = self.assets / 'SHA256SUMS'
        lines = sums.read_text().splitlines()
        sums.write_text(''.join((sha(archive) + '  ' + archive.name if line.endswith(archive.name) else line) + '\n' for line in lines))
        self.receipt['checksums_sha256'] = sha(sums)
        with self.assertRaisesRegex(ValueError, 'archive checksum'):
            self.verify()

    def test_checksum_gate_binds_archive_transport_bytes(self):
        self.envelope()
        archive = self.assets / 'Buzz_0.5.26_aarch64.app.tar.gz'
        content = bytearray(archive.read_bytes())
        content[4] ^= 1  # Only gzip timestamp differs; unpacked app bytes still match.
        archive.write_bytes(content)
        with self.assertRaisesRegex(ValueError, 'artifact checksum'):
            self.verify()

    def add_archive_metadata(self, name, payload):
        archive = self.assets / 'Buzz_0.5.26_aarch64.app.tar.gz'
        rewritten = self.assets / 'metadata.tar.gz'
        with tarfile.open(archive, 'r:gz') as source, tarfile.open(rewritten, 'w:gz') as target:
            for member in source:
                target.addfile(member, source.extractfile(member) if member.isfile() else None)
            member = tarfile.TarInfo(name)
            member.size = len(payload)
            member.mode = 0o755
            target.addfile(member, io.BytesIO(payload))
        rewritten.replace(archive)
        sums = self.assets / 'SHA256SUMS'
        lines = sums.read_text().splitlines()
        sums.write_text(''.join((sha(archive) + '  ' + archive.name if line.endswith(archive.name) else line) + '\n' for line in lines))
        self.receipt['checksums_sha256'] = sha(sums)

    def test_macos_producer_appledouble_metadata_is_not_an_extra_binary(self):
        self.envelope()
        metadata = struct.pack('>II16sHIII', 0x00051607, 0x00020000, bytes(16), 1, 9, 38, 32) + bytes(32)
        for name in ('._Buzz.app', 'Buzz.app/Contents/MacOS/._buzz-acp'):
            self.add_archive_metadata(name, metadata)
        try:
            result = self.verify()
        except ValueError as error:
            self.fail('M3 BSD-tar metadata must preserve real app verification: ' + str(error))
        self.assertEqual(result.candidate_sha, self.candidate)

    def test_appledouble_name_cannot_hide_unassociated_or_arbitrary_payload(self):
        self.envelope()
        metadata = struct.pack('>II16sHIII', 0x00051607, 0x00020000, bytes(16), 1, 9, 38, 32) + bytes(32)
        archive = self.assets / 'Buzz_0.5.26_aarch64.app.tar.gz'
        sums = self.assets / 'SHA256SUMS'
        original_archive, original_sums = archive.read_bytes(), sums.read_bytes()
        original_pin = self.receipt['checksums_sha256']
        cases = [('Buzz.app/Contents/MacOS/._missing', metadata),
                 ('Buzz.app/Contents/MacOS/../._buzz-acp', metadata),
                 ('Buzz.app/Contents/MacOS//._buzz-acp', metadata),
                 ('._Buzz.app', b'#!/bin/sh\nmalicious executable payload'),
                 ('._Buzz.app', b'EXECBIN!' + metadata[8:]),
                 ('._Buzz.app', metadata[:26]),
                 ('._Buzz.app', metadata[:30] + (1000).to_bytes(4, 'big') + metadata[34:]),
                 ('._Buzz.app', metadata + bytes(1024*1024)),
                 ('Buzz.app/Contents/MacOS/buzz-acp', metadata)]
        for name, payload in cases:
            with self.subTest(name=name, size=len(payload)):
                archive.write_bytes(original_archive)
                sums.write_bytes(original_sums)
                self.receipt['checksums_sha256'] = original_pin
                self.add_archive_metadata(name, payload)
                with self.assertRaises(ValueError):
                    self.verify()

    def test_missing_baseline_or_changed_identity_rejects(self):
        self.envelope()
        self.manifest['commit_sha'] = self.merged
        with self.assertRaises(ValueError):
            self.verify()

    def test_cli_requires_acceptance_and_never_mutates_by_default(self):
        self.envelope()
        path = self.root / 'acceptance.json'
        path.write_text(json.dumps(self.receipt))
        argv = ['python3', str(MODULE), '--repo', str(self.repo), '--candidate', self.candidate,
                '--merged', self.merged, '--manifest', str(self.assets / 'manifest.json'), '--fork-revision', '1']
        result = subprocess.run(argv, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        result = subprocess.run([*argv, '--acceptance', str(path)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['tag'], 'fork-v0.5.26-1')
        self.assertEqual(self.git('tag', '--list', 'fork-v*'), '')

    def publication_fixture(self):
        self.envelope()
        self.origin = self.root / 'origin.git'
        subprocess.run([GIT, 'init', '--bare', '-q', str(self.origin)], check=True)
        self.git('remote', 'add', 'origin', str(self.origin))
        self.git('push', '-q', 'origin', 'fork/main', 'fork/sync-v0.5.26')
        identity = {'full_name': 'reynholm/buzz'}
        pr = {'merged': True, 'merged_at': '2026-10-04T00:00:00Z', 'draft': False,
              'merged_by': {'login': 'reynholm'}, 'merge_commit_sha': self.merged,
              'head': {'sha': self.candidate, 'ref': 'fork/sync-v0.5.26', 'repo': identity},
              'base': {'ref': 'fork/main', 'repo': identity}}
        self.state_path = self.root / 'gh.json'
        self.state_path.write_text(json.dumps({'pr': pr, 'release': None, 'calls': []}))
        bin_dir = self.root / 'bin'
        bin_dir.mkdir()
        gh = bin_dir / 'gh'
        gh.write_text('#!' + sys.executable + '\nimport sys,os\nfrom pathlib import Path\n'
                      + 'sys.path.insert(0, ' + repr(str(Path(__file__).parent)) + ')\n'
                      + 'from test_promote import gh_fixture\n'
                      + 'print(gh_fixture(Path(os.environ["GH_FIXTURE_ROOT"]),sys.argv[1:]))\n')
        gh.chmod(0o755)
        self.env = {**os.environ, 'GH_FIXTURE_ROOT': str(self.root),
                    'PATH': str(bin_dir) + os.pathsep + os.environ['PATH']}
        path = self.root / 'acceptance.json'
        path.write_text(json.dumps(self.receipt))
        self.argv = [sys.executable, str(MODULE), '--repo', str(self.repo),
                     '--candidate', self.candidate, '--merged', self.merged,
                     '--manifest', str(self.assets / 'manifest.json'), '--fork-revision', '1',
                     '--acceptance', str(path), '--publish', '--fixture']

    def cli_publish(self):
        return subprocess.run(self.argv, env=self.env, capture_output=True, text=True)

    def remote_git(self, *args):
        return subprocess.check_output([GIT, '-C', str(self.origin), *args], text=True)

    def test_cli_end_to_end_publication_and_idempotent_resume(self):
        self.publication_fixture()
        before = self.git('rev-parse', 'HEAD').strip()
        for attempt in range(2):
            result = self.cli_publish()
            self.assertEqual(result.returncode, 0, result.stderr)
            document = json.loads(result.stdout)
            self.assertEqual(document['state'], 'published')
            self.assertEqual(document['candidate_sha'], self.candidate)
            self.assertEqual(self.remote_git('rev-parse', 'fork-v0.5.26-1').strip(), self.merged)
            self.assertEqual(self.remote_git('rev-parse', 'fork-v0.5.26-1^{tree}').strip(), self.tree)
            self.assertEqual(self.remote_git('branch', '--list', 'fork/sync-*'), '')
            self.assertEqual(self.git('rev-parse', 'HEAD').strip(), before)
        state = json.loads(self.state_path.read_text())
        self.assertFalse(state['release']['draft'])
        self.assertEqual(sum(call[:2] == ['release', 'create'] for call in state['calls']), 1)
        for path in self.assets.iterdir():
            self.assertEqual(sha(path), sha(self.root / 'remote-assets' / path.name))
        self.assertIn(self.candidate, state['release']['body'])

    def test_authoritative_owner_merge_rejection_precedes_all_mutations(self):
        self.publication_fixture()
        original = json.loads(self.state_path.read_text())
        for key, value in [('merged', False), ('merged_by', {'login': 'automation'}),
                           ('merge_commit_sha', self.candidate)]:
            state = copy.deepcopy(original)
            state['pr'][key] = value
            self.state_path.write_text(json.dumps(state))
            result = self.cli_publish()
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('owner-merged', result.stderr)
            self.assertEqual(self.remote_git('tag', '--list', 'fork-v*'), '')
            self.assertIsNone(json.loads(self.state_path.read_text())['release'])
            self.assertNotEqual(self.remote_git('branch', '--list', 'fork/sync-*'), '')

    def test_remote_conflicting_tag_rejects_before_release(self):
        self.publication_fixture()
        self.git('tag', 'fork-v0.5.26-1', self.candidate)
        self.git('push', '-q', 'origin', 'fork-v0.5.26-1')
        self.git('tag', '-d', 'fork-v0.5.26-1')
        result = self.cli_publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('tag', result.stderr)
        self.assertIsNone(json.loads(self.state_path.read_text())['release'])

    def test_remote_annotated_tag_same_commit_can_resume(self):
        self.publication_fixture()
        self.git('-c', 'tag.gpgsign=false', 'tag', '-am', 'synthetic accepted annotated tag',
                 'fork-v0.5.26-1', self.merged)
        self.git('push', '-q', 'origin', 'fork-v0.5.26-1')
        result = self.cli_publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.remote_git('rev-parse', 'fork-v0.5.26-1^{commit}').strip(), self.merged)

    def test_old_identical_release_resumes_beyond_first_api_page(self):
        self.publication_fixture()
        self.assertEqual(self.cli_publish().returncode, 0)
        state = json.loads(self.state_path.read_text())
        state['paginate_needed'] = True
        self.state_path.write_text(json.dumps(state))
        result = self.cli_publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        state = json.loads(self.state_path.read_text())
        self.assertEqual(sum(call[:2] == ['release', 'create'] for call in state['calls']), 1)

    def test_resume_rejects_remote_asset_tampering_without_branch_cleanup(self):
        self.publication_fixture()
        self.assertEqual(self.cli_publish().returncode, 0)
        self.git('push', '-q', 'origin', 'fork/sync-v0.5.26')
        (self.root / 'remote-assets/manifest.json').write_text('{}')
        result = self.cli_publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('checksum', result.stderr)
        self.assertNotEqual(self.remote_git('branch', '--list', 'fork/sync-*'), '')

    def test_synthetic_receipt_cannot_publish_to_network_origin(self):
        self.publication_fixture()
        self.git('remote', 'set-url', 'origin', 'https://github.com/reynholm/buzz.git')
        result = self.cli_publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(json.loads(self.state_path.read_text())['calls'], [])

    def test_assets_are_pinned_before_external_publication_calls(self):
        self.publication_fixture()
        state = json.loads(self.state_path.read_text())
        state['tamper_on_pr'] = str(self.assets / 'manifest.json')
        self.state_path.write_text(json.dumps(state))
        result = self.cli_publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sha(self.root / 'remote-assets/manifest.json'), self.receipt['manifest_sha256'])

    def test_concurrent_sync_branch_replacement_is_preserved(self):
        self.publication_fixture()
        hook = self.origin / 'hooks/pre-receive'
        hook.write_text('#!/bin/sh\nwhile read old new ref; do\n'
                        + 'if [ "$new" = "0000000000000000000000000000000000000000" ]; then\n'
                        + '"' + GIT + '" update-ref "$ref" ' + self.merged + '\nfi\ndone\n')
        hook.chmod(0o755)
        result = self.cli_publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.remote_git('rev-parse', 'fork/sync-v0.5.26').strip(), self.merged)
        self.assertEqual(self.remote_git('rev-parse', 'fork-v0.5.26-1').strip(), self.merged)
        self.assertFalse(json.loads(self.state_path.read_text())['release']['draft'])

    def test_owner_manual_workflow_has_no_automatic_merge_or_promotion(self):
        path = MODULE.parents[2] / '.github/workflows/fork-promote.yml'
        self.assertTrue(path.is_file(), 'manual owner-gated promotion workflow is missing')
        workflow = json.loads(path.read_text())
        self.assertEqual(set(workflow['on']), {'workflow_dispatch'})
        job = workflow['jobs']['promote']
        self.assertEqual(job['environment'], 'fork-promotion')
        commands = '\n'.join(step.get('run', '') for step in job['steps'])
        self.assertIn('scripts/fork/promote.py', commands)
        self.assertIn('--acceptance', commands)
        self.assertIn('--publish', commands)
        self.assertNotIn('pr merge', commands)
        self.assertIn('python3 -m unittest discover -s scripts/fork/tests -v', commands)

    def test_manual_workflow_receipt_step_writes_valid_json_and_environment(self):
        path = MODULE.parents[2] / '.github/workflows/fork-promote.yml'
        workflow = json.loads(path.read_text())
        step = next(s for s in workflow['jobs']['promote']['steps'] if s.get('name') == 'Retain owner receipt and select one manifest')
        root = self.root / 'workflow'
        root.mkdir()
        assets = root / 'candidate-assets'
        assets.mkdir()
        (assets / 'manifest.json').write_text('{}')
        env_path = root / 'environment'
        receipt = {'synthetic': False, 'reference': 'synthetic step fixture; no publication executed'}
        env = {**os.environ, 'ACCEPTANCE_JSON': json.dumps(receipt), 'GITHUB_ENV': str(env_path)}
        result = subprocess.run(['bash', '-e', '-c', step['run']], cwd=root, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads((root / 'owner-acceptance.json').read_text()), receipt)
        self.assertEqual(env_path.read_text(), 'MANIFEST_PATH=candidate-assets/manifest.json\n')
        env['ACCEPTANCE_JSON'] = json.dumps({'synthetic': True})
        result = subprocess.run(['bash', '-e', '-c', step['run']], cwd=root, env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('synthetic', result.stderr)

    def test_draft_body_contains_handoff_requirements(self):
        report = sync.SyncReport('ready', base_sha=self.candidate, target_sha=self.base,
                                 candidate_sha=self.candidate, target_tag='desktop-v0.5.26',
                                 prior_base={'base_tag': 'desktop-v0.5.25', 'base_sha': 'a'*40})
        self.assertTrue(hasattr(sync, 'draft_body'), 'reviewable draft body is missing')
        body = sync.gh_body(['gh', 'pr', 'create'], report, lambda argv: Path(argv[-1]).read_text())
        for text in (self.candidate, self.base, 'desktop-v0.5.25', 'releases/tag/desktop-v0.5.26',
                     'persona', 'sync', 'SHA256SUMS', 'two physical devices', 'full tests', 'conflicts'):
            self.assertIn(text, body)


if __name__ == '__main__':
    unittest.main()
