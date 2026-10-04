"""Synthetic releases in disposable repositories exercise real Git preparation."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from test_patch_registry import GIT, MODULE, sync


class SyncTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name) / 'repo'
        self.repo.mkdir()
        # Disposable identities are self-contained even on a clean CI runner.
        global_config = Path(self.temp.name) / 'gitconfig'
        global_config.write_text('[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n')
        env = patch.dict(os.environ, {'GIT_CONFIG_GLOBAL': str(global_config),
                                      'GIT_CONFIG_NOSYSTEM': '1'})
        env.start()
        self.addCleanup(env.stop)
        self.git('init', '-q')
        # Match the required global author guard; this only changes the fixture config.
        self.git('config', 'user.name', self.git('config', '--global', 'user.name').strip())
        self.git('config', 'user.email', self.git('config', '--global', 'user.email').strip())
        self.caller_base = 'def run():\n    guard()\n' + '# padding\n' * 12
        (self.repo / 'caller.py').write_text(self.caller_base)
        (self.repo / 'upstream.txt').write_text('old\n')
        self.commit('upstream base', ['caller.py', 'upstream.txt'])
        self.upstream_base = self.git('rev-parse', 'HEAD').strip()
        self.git('tag', 'desktop-v0.5.26')
        self.git('branch', 'upstream')
        self.git('checkout', '-qb', 'fork/main')
        (self.repo / 'caller.py').write_text(self.caller_base + '# fork\n')
        paths = ['caller.py', 'FORK_PATCHES.md', 'scripts/fork/patches.json']
        self.manifest = {'base_tag': 'desktop-v0.5.26', 'base_sha': self.upstream_base,
                         'patches': [self.entry(p) for p in paths]}
        self.manifest['patches'][0]['required_symbols'] = ['guard()']
        self.manifest['patches'][0]['invocation_seams'] = [{
            'caller_path': 'caller.py', 'callee_symbol': 'guard', 'invocation': 'guard()',
            'behavior_test': 'synthetic_execution', 'verification_command': 'python3 -m unittest'}]
        (self.repo / 'scripts/fork').mkdir(parents=True)
        self.write_manifest()
        self.commit('fork patches', paths)
        self.base = self.git('rev-parse', 'HEAD').strip()
        self.git('checkout', '-q', 'upstream')
        (self.repo / 'upstream.txt').write_text('new upstream-only change\n')
        self.commit('new upstream', ['upstream.txt'])
        self.target = self.git('rev-parse', 'HEAD').strip()
        self.git('tag', 'desktop-v0.5.27')
        self.git('checkout', '-q', 'fork/main')
        self.evidence = {'upstream_sha': self.target, 'gate_commands': ['synthetic gate'],
                         'result': 'success', 'artifact_manifest': {'synthetic': True}}

    def entry(self, path):
        return {'path': path, 'responsibility': 'Synthetic fixture patch',
                'is_new': path != 'caller.py', 'required_symbols': [],
                'invocation_seams': [], 'verification_commands': ['python3 -m unittest']}

    def git(self, *args):
        return subprocess.check_output([GIT, '-C', str(self.repo), *args], text=True)

    def commit(self, message, paths):
        self.git('add', '--', *paths)
        self.git('-c', 'commit.gpgsign=false', 'commit', '-qsm', message)

    def write_manifest(self):
        (self.repo / 'scripts/fork/patches.json').write_text(json.dumps(self.manifest, indent=2) + '\n')
        (self.repo / 'FORK_PATCHES.md').write_text(sync.render_registry(self.manifest))

    def test_production_commit_guard_rejects_missing_author_or_signing(self):
        for key, value in [('user.email', ''), ('commit.gpgsign', 'true')]:
            with self.subTest(key=key):
                self.git('config', key, value)
                with self.assertRaisesRegex(RuntimeError, 'configured author/email'):
                    sync.commit_owned(self.repo, [], 'must not commit')
                self.git('config', '--unset', key)
                self.git('config', 'user.email', 'fixture@example.invalid')

    @unittest.skipUnless(sync.IDENTITY_WRAPPER.is_file(), 'local wrapper guard only')
    def test_production_commit_guard_rejects_local_global_mismatch(self):
        self.git('config', 'user.name', 'Different fixture')
        with self.assertRaisesRegex(RuntimeError, 'trusted global identity guard'):
            sync.commit_owned(self.repo, [], 'must not commit')

    def selection(self):
        self.assertTrue(hasattr(sync, 'select_update'), 'release selection is missing')
        return sync.select_update(['desktop-v0.5.27'], 'desktop-v0.5.26')

    def prepare(self):
        self.assertTrue(hasattr(sync, 'prepare_update'), 'guarded preparation is missing')
        return sync.prepare_update(self.repo, self.selection(), self.manifest,
                                   clean_target=self.evidence)

    def test_semver_selection_ignores_nonrelease_tags(self):
        self.assertTrue(hasattr(sync, 'select_update'), 'release selection is missing')
        selection = sync.select_update(['mobile-v99.0.0', 'desktop-v0.5.28-rc1',
                                        'desktop-v0.5.27', 'desktop-v0.10.0',
                                        'desktop-v0.6.0'], 'desktop-v0.5.26')
        self.assertEqual(selection.target_tag, 'desktop-v0.10.0')
        self.assertEqual(selection.skipped_tags, ['desktop-v0.5.27', 'desktop-v0.6.0'])
        self.assertIsNone(sync.select_update(['desktop-v0.5.26'], 'desktop-v0.5.26'))

    def test_no_update_is_silent(self):
        # CLI no-update must exit cleanly, write structured evidence and emit nothing.
        self.git('tag', '-d', 'desktop-v0.5.27')
        report = Path(self.temp.name) / 'no-update.json'
        result = subprocess.run(['python3', str(MODULE), 'prepare', '--repo', str(self.repo),
                                 '--offline', '--report', str(report)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, '')
        self.assertEqual(json.loads(report.read_text())['state'], 'no_update')
        self.assertEqual(self.git('branch', '--list', 'fork/sync-*'), '')

    def test_cli_prepare_runs_guarded_candidate(self):
        baseline = Path(self.temp.name) / 'baseline.json'
        baseline.write_text(json.dumps(self.evidence))
        report = Path(self.temp.name) / 'preparation.json'
        result = subprocess.run(['python3', str(MODULE), 'prepare', '--repo', str(self.repo),
                                 '--offline', '--baseline-report', str(baseline), '--report', str(report)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        document = json.loads(report.read_text())
        self.assertEqual(document['state'], 'ready')
        self.assertEqual(document['target_sha'], self.target)
        self.assertEqual(self.git('rev-parse', document['branch']).strip(), document['candidate_sha'])

    def test_same_version_reuses_branch_and_pr(self):
        first = self.prepare()
        self.assertEqual(first.state, 'ready', first)
        second = self.prepare()
        self.assertEqual(second.branch, first.branch)
        self.assertEqual(second.candidate_sha, first.candidate_sha)
        self.assertTrue(hasattr(sync, 'publish_draft'), 'draft publication is missing')
        calls, prs = [], []

        def gh(argv):
            calls.append(argv)
            if argv[:3] == ['gh', 'pr', 'list']:
                return json.dumps(prs)
            if argv[:3] == ['gh', 'pr', 'create']:
                self.assertIn('--draft', argv)
                prs.append({'url': 'https://github.com/reynholm/buzz/pull/123'})
                return prs[0]['url']
            raise AssertionError(argv)

        pushes = []
        with patch.object(sync, 'push_branch', side_effect=lambda *args: pushes.append(args)):
            one = sync.publish_draft(self.repo, first, gh=gh)
            two = sync.publish_draft(self.repo, second, gh=gh)
        self.assertEqual(one, two)
        self.assertEqual(sum(c[:3] == ['gh', 'pr', 'create'] for c in calls), 1)
        self.assertEqual(len(pushes), 2)
        with patch.object(sync, 'git') as runner:
            sync.push_branch(self.repo, first.branch)
        self.assertNotIn('--force', repr(runner.call_args))
        self.assertNotIn('--force-with-lease', repr(runner.call_args))

    def test_clean_merge_uses_target_registry_and_preserves_patch(self):
        report = self.prepare()
        self.assertEqual(report.state, 'ready', report)
        self.assertEqual(report.prior_base['base_sha'], self.upstream_base)
        self.assertEqual(report.target_sha, self.target)
        self.assertEqual(self.git('show', f'{report.branch}:upstream.txt'), 'new upstream-only change\n')
        self.assertIn('guard()', self.git('show', f'{report.branch}:caller.py'))
        manifest = json.loads(self.git('show', f'{report.branch}:scripts/fork/patches.json'))
        self.assertEqual(manifest['base_sha'], self.target)
        self.assertEqual(self.git('rev-parse', 'fork/main').strip(), self.base)
        self.assertEqual(self.git('status', '--porcelain'), '')
        self.assertEqual(self.git('rev-list', '--parents', '-n', '1', report.candidate_sha).split()[1:],
                         [self.base, self.target])

    def test_merge_success_missing_seam_blocks(self):
        self.git('checkout', '-q', 'upstream')
        (self.repo / 'caller.py').write_text(self.caller_base.replace('guard()', 'pass'))
        self.commit('upstream removes guard', ['caller.py'])
        self.git('tag', '-f', 'desktop-v0.5.27')
        self.target = self.git('rev-parse', 'HEAD').strip()
        self.evidence['upstream_sha'] = self.target
        self.git('checkout', '-q', 'fork/main')
        report = self.prepare()
        self.assertEqual(report.state, 'blocked')
        self.assertIn('missing invocation', '\n'.join(report.missing_seams))
        self.assertIsNone(report.candidate_sha)

    def conflict(self):
        self.git('checkout', '-q', 'upstream')
        (self.repo / 'caller.py').write_text('upstream incompatible replacement\n')
        self.commit('upstream conflict', ['caller.py'])
        self.git('tag', '-f', 'desktop-v0.5.27')
        self.target = self.git('rev-parse', 'HEAD').strip()
        self.evidence['upstream_sha'] = self.target
        self.git('checkout', '-q', 'fork/main')
        return self.prepare()

    def test_conflict_reports_without_overwriting_patch(self):
        original = (self.repo / 'caller.py').read_text()
        report = self.conflict()
        self.assertEqual(report.state, 'blocked')
        self.assertEqual(report.conflicts, ['caller.py'])
        self.assertEqual((self.repo / 'caller.py').read_text(), original)
        self.assertEqual(self.git('status', '--porcelain'), '')
        self.assertIsNone(report.candidate_sha)

    def test_unmerged_index_produces_report_only_draft(self):
        report = self.conflict()
        self.assertTrue(hasattr(sync, 'prepare_blocked_report'), 'clean blocked report is missing')
        sha = sync.prepare_blocked_report(self.repo, self.selection(), report)
        repeated = sync.prepare_blocked_report(self.repo, self.selection(), report)
        self.assertEqual(sha, repeated)
        diff = self.git('diff', '--name-only', self.base, sha).splitlines()
        self.assertEqual(diff, ['docs/fork/sync-reports/desktop-v0.5.27.json'])
        document = json.loads(self.git('show', f'{sha}:{diff[0]}'))
        self.assertEqual(document['state'], 'blocked')
        self.assertEqual(document['conflicts'], ['caller.py'])
        self.assertEqual(document['base_sha'], self.base)
        self.assertEqual(document['target_sha'], self.target)
        self.assertNotIn(str(self.repo), json.dumps(document))
        self.assertIn('guard()', self.git('show', f'{sha}:caller.py'))

    def test_clean_target_failure_prevents_candidate(self):
        self.evidence['result'] = 'failure'
        report = self.prepare()
        self.assertEqual(report.state, 'blocked')
        self.assertEqual(self.git('branch', '--list', 'fork/sync-*'), '')
        self.assertIsNone(report.candidate_sha)

    def test_stale_or_absent_baseline_is_blocked(self):
        self.evidence['upstream_sha'] = self.upstream_base
        self.assertEqual(self.prepare().state, 'blocked')
        self.evidence = None
        self.assertEqual(self.prepare().state, 'blocked')

    def test_foreign_branch_is_not_deleted(self):
        self.git('branch', 'fork/sync-v0.5.27')
        report = self.prepare()
        self.assertEqual(report.state, 'blocked')
        self.assertIn('ownership', report.reason)
        self.assertEqual(self.git('rev-parse', 'fork/sync-v0.5.27').strip(), self.base)

    def test_interrupted_preparation_resumes(self):
        self.assertTrue(hasattr(sync, 'merge_target'), 'merge seam is missing')
        with patch.object(sync, 'merge_target', side_effect=RuntimeError('synthetic interruption')):
            with self.assertRaisesRegex(RuntimeError, 'interruption'):
                self.prepare()
        report = self.prepare()
        self.assertEqual(report.state, 'ready', report)

    def test_changed_base_stops_resume(self):
        first = self.prepare()
        (self.repo / 'caller.py').write_text('def run():\n    guard()\n# advanced accepted base\n')
        self.commit('advance fork/main', ['caller.py'])
        report = self.prepare()
        self.assertEqual(report.state, 'blocked')
        self.assertIn('base moved', report.reason)
        self.assertEqual(self.git('rev-parse', first.branch).strip(), first.candidate_sha)

    def test_new_runner_reuses_published_candidate_without_force(self):
        first = self.prepare()
        remote = Path(self.temp.name) / 'remote.git'
        subprocess.run([GIT, 'init', '--bare', '-q', str(remote)], check=True)
        self.git('remote', 'add', 'origin', str(remote))
        self.git('push', '-q', '--tags', 'origin', 'fork/main', first.branch)
        fresh = Path(self.temp.name) / 'fresh'
        subprocess.run([GIT, 'clone', '-q', '--branch', 'fork/main', str(remote), str(fresh)], check=True)
        self.assertTrue(hasattr(sync, 'reuse_published_candidate'), 'cross-runner reuse is missing')
        reused = sync.reuse_published_candidate(fresh, self.selection(), self.manifest,
                                                clean_target=self.evidence)
        self.assertEqual(reused.candidate_sha, first.candidate_sha)
        self.assertEqual(reused.branch, first.branch)
        self.assertEqual(reused.state, 'ready')
        self.assertEqual(subprocess.check_output([GIT, '-C', str(fresh), 'status', '--porcelain'], text=True), '')

    def test_real_clean_target_gate_failure_is_recorded_before_later_build(self):
        self.assertTrue(hasattr(sync, 'run_clean_target'), 'clean target execution is missing')
        evidence_path = Path(self.temp.name) / 'baseline.json'
        marker = Path(self.temp.name) / 'forbidden-build'
        commands = [['python3', '-c', 'raise SystemExit(7)'],
                    ['python3', '-c', 'import pathlib,sys; pathlib.Path(sys.argv[1]).touch()', str(marker)]]
        with patch.object(sync, 'BASELINE_GATES', commands):
            with self.assertRaises(subprocess.CalledProcessError):
                sync.run_clean_target(self.repo, self.target, evidence_path)
        self.assertFalse(marker.exists())
        self.evidence = json.loads(evidence_path.read_text())
        self.assertEqual(self.evidence['upstream_sha'], self.target)
        self.assertEqual(self.evidence['result'], 'failure')
        self.assertEqual(self.evidence['artifact_manifest'], {})
        self.assertEqual(self.prepare().state, 'blocked')
        self.assertEqual(self.git('branch', '--list', 'fork/sync-*'), '')

    def test_staged_conflict_markers_are_never_buildable(self):
        report = self.conflict()
        worktree = sync.state_directory(self.repo) / 'worktrees' / report.target_tag
        # Simulate an interrupted manual repair accidentally staging conflict text.
        (worktree / 'caller.py').write_text('<<<<<<< HEAD\ndef run():\n    guard()\n=======\nother\n>>>>>>> upstream\n')
        subprocess.run([GIT, '-C', str(worktree), 'add', '--', 'caller.py'], check=True)
        retried = self.prepare()
        self.assertEqual(retried.state, 'blocked')
        self.assertIsNone(retried.candidate_sha)

    def test_new_runner_reuses_clean_blocked_report(self):
        report = self.conflict()
        first_sha = sync.prepare_blocked_report(self.repo, self.selection(), report)
        remote = Path(self.temp.name) / 'remote.git'
        subprocess.run([GIT, 'init', '--bare', '-q', str(remote)], check=True)
        self.git('remote', 'add', 'origin', str(remote))
        self.git('push', '-q', '--tags', 'origin', 'fork/main', report.report_branch)
        fresh = Path(self.temp.name) / 'fresh'
        subprocess.run([GIT, 'clone', '-q', '--branch', 'fork/main', str(remote), str(fresh)], check=True)
        fresh_report = sync.prepare_update(fresh, self.selection(), self.manifest, clean_target=self.evidence)
        self.assertEqual(fresh_report.state, 'blocked')
        sync.reuse_published_report(fresh, self.selection(), fresh_report)
        self.assertEqual(sync.prepare_blocked_report(fresh, self.selection(), fresh_report), first_sha)

    def test_checked_out_upstream_mirror_is_never_moved(self):
        other = Path(self.temp.name) / 'foreign-upstream'
        self.git('worktree', 'add', '-q', str(other), 'upstream')
        # Advance selected tag beyond the mirror while preserving foreign checkout.
        self.git('checkout', '-qb', 'new-target', 'desktop-v0.5.27')
        (self.repo / 'upstream.txt').write_text('newer\n')
        self.commit('newer target', ['upstream.txt'])
        self.git('tag', '-f', 'desktop-v0.5.27')
        self.target = self.git('rev-parse', 'HEAD').strip()
        self.evidence['upstream_sha'] = self.target
        self.git('checkout', '-q', 'fork/main')
        mirror = self.git('rev-parse', 'upstream').strip()
        report = self.prepare()
        self.assertEqual(report.state, 'blocked')
        self.assertEqual(self.git('rev-parse', 'upstream').strip(), mirror)


if __name__ == '__main__':
    unittest.main()
