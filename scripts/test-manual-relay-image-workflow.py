#!/usr/bin/env python3
"""Exercise the actual manual-image resolver shell without network or publishing."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / '.github/workflows/docker-manual.yml'
GIT = shutil.which('git')
if GIT is None:
    raise RuntimeError('git is required for the isolated resolver fixture')


def shell_step(prefix):
    text = WORKFLOW.read_text()
    steps = re.finditer(r'^      - name: ([^\n]+)\n(.*?)(?=^      - |^  [a-z]|\Z)', text, re.M | re.S)
    for step in steps:
        if step[1].startswith(prefix):
            body = step[2].split('        run: |\n', 1)[1]
            return '\n'.join(line[10:] for line in body.splitlines() if line.startswith('          ')) + '\n'
    raise AssertionError(f'missing production shell step: {prefix}')


class ManualImageScope(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name)
        self.env = {'PATH': os.environ['PATH'], 'REPOSITORY': 'reynholm/buzz',
                    'ACTOR': 'reynholm', 'TRIGGERING_ACTOR': 'reynholm',
                    'SOURCE_REPOSITORY': 'reynholm/buzz', 'IMAGE_NAME': 'ghcr.io/reynholm/buzz',
                    'GITHUB_RUN_ID': '123', 'GITHUB_RUN_ATTEMPT': '2',
                    'GITHUB_OUTPUT': str(self.path / 'output')}

    def run_shell(self, script, **changes):
        return subprocess.run(['bash', '-c', script], env=self.env | changes,
                              text=True, capture_output=True, timeout=10)

    def test_exact_authorized_scope_and_all_foreign_destinations(self):
        guard = shell_step('Require ')
        self.assertEqual(self.run_shell(guard).returncode, 0)
        for changes in [{'REPOSITORY': 'block/buzz'}, {'REPOSITORY': 'another/fork'},
                        {'ACTOR': 'another'}, {'TRIGGERING_ACTOR': 'another'},
                        {'SOURCE_REPOSITORY': 'block/buzz'},
                        {'IMAGE_NAME': 'ghcr.io/block/buzz'},
                        {'IMAGE_NAME': 'ghcr.io/reynholm/another'}]:
            with self.subTest(changes=changes):
                self.assertNotEqual(self.run_shell(guard, **changes).returncode, 0)

    def test_job_gate_and_writable_namespaces_are_closed(self):
        text = WORKFLOW.read_text()
        self.assertIn("if: github.repository == 'reynholm/buzz' && github.actor == 'reynholm' && github.triggering_actor == 'reynholm'", text)
        self.assertIn('SOURCE_REPOSITORY: reynholm/buzz', text)
        self.assertIn('IMAGE_NAME: ghcr.io/reynholm/buzz', text)
        self.assertNotIn('vars.GHCR_IMAGE', text)
        self.assertIn('needs: resolve', text)
        self.assertIn('needs: [resolve, build]', text)
        self.assertIn('cache-to: type=registry,ref=${{ env.IMAGE_NAME }}-buildcache:manual-${{ matrix.arch }}', text)
        self.assertIn('ci_qualification:"skipped"', text)
        self.assertIn('No deployment was started.', text)
        self.assertNotIn('workflow_call:', text)

    def test_source_refs_and_reachability_with_real_git(self):
        seed = self.path / 'seed'
        seed.mkdir()
        def git(*args):
            return subprocess.check_output([GIT, '-C', str(seed), *args], text=True, stderr=subprocess.DEVNULL).strip()
        git('init', '-q', '-b', 'main')
        # Disposable fixture identities certify no outgoing source commits.
        git('config', 'user.name', 'Fixture')
        git('config', 'user.email', 'fixture@example.invalid')
        git('config', 'commit.gpgsign', 'false')
        (seed / 'source').write_text('first')
        git('add', 'source'); git('commit', '-qm', 'Fixture main')
        main = git('rev-parse', 'HEAD'); git('tag', 'on-main')
        git('checkout', '-q', '--orphan', 'temporary')
        (seed / 'source').write_text('orphan')
        git('add', 'source'); git('commit', '-qm', 'Fixture PR-only')
        orphan = git('rev-parse', 'HEAD'); git('tag', 'pr-only')
        git('checkout', '-q', 'main'); git('branch', '-D', 'temporary')
        bindir = self.path / 'bin'; bindir.mkdir()
        adapter = bindir / 'git'
        adapter.write_text('\n'.join([
            '#!/usr/bin/env python3', 'import os,sys',
            'a=sys.argv[1:]',
            "os.environ['PATH']=os.environ['FIXTURE_PATH']",
            "if len(a)>5 and a[2:5]==['remote','add','origin']:",
            " assert a[5]=='https://github.com/reynholm/buzz.git', a[5]",
            " a[5]=os.environ['FIXTURE_REMOTE']",
            "os.execv(os.environ['FIXTURE_GIT'], [os.environ['FIXTURE_GIT'],*a])", '']))
        adapter.chmod(0o755)
        env = dict(PATH=str(bindir) + os.pathsep + self.env['PATH'],
                   FIXTURE_REMOTE=str(seed), FIXTURE_GIT=GIT, FIXTURE_PATH=self.env['PATH'])
        resolver = shell_step('Resolve a source')
        for ref in ['main', 'refs/heads/main', 'on-main', 'refs/tags/on-main', main]:
            with self.subTest(ref=ref):
                (self.path / 'output').unlink(missing_ok=True)
                result = self.run_shell(resolver, SOURCE_REF=ref, **env)
                self.assertEqual(result.returncode, 0, result.stderr)
                output = (self.path / 'output').read_text()
                self.assertIn('sha=' + main, output)
                self.assertIn('tag=manual-' + main[:12] + '-123-2', output)
        for ref in ['refs/pull/1/head', 'pull/1/head', '../main', '-main', 'x.lock',
                    'main;touch nope', '', 'pr-only', orphan]:
            with self.subTest(ref=ref):
                (self.path / 'output').unlink(missing_ok=True)
                result = self.run_shell(resolver, SOURCE_REF=ref, **env)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertFalse((self.path / 'output').exists())


if __name__ == '__main__':
    unittest.main()
