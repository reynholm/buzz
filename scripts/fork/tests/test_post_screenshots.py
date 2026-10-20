"""Exercise screenshot publication against a local bare fork, never GitHub."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[2] / 'post-screenshots.sh'


class ScreenshotPublicationTests(unittest.TestCase):
    def test_explicit_fork_preserves_remote_history_and_signs_off(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkout, remote, tools = root / 'checkout', root / 'fork.git', root / 'bin'
            checkout.mkdir()
            tools.mkdir()
            env = {key: value for key, value in os.environ.items()
                   if not key.startswith(('GIT_CONFIG_', 'GIT_AUTHOR_', 'GIT_COMMITTER_'))}
            env.update(GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1',
                       PATH=str(tools) + os.pathsep + env['PATH'], GH_CAPTURE=str(root / 'gh.json'))

            def git(*args, cwd=checkout):
                return subprocess.check_output(['git', *args], cwd=cwd, env=env, text=True).strip()

            git('init', '--bare', str(remote))
            git('init')
            git('config', 'user.name', 'Synthetic Contributor')
            git('config', 'user.email', 'contributor@example.invalid')
            git('remote', 'add', 'fork', str(remote))
            real_git = shutil.which('git')
            env.update(REAL_GIT=real_git, GIT_CAPTURE=str(root / 'git.jsonl'))
            (tools / 'git').write_text(
                '#!/usr/bin/env python3\nimport json,os,sys\n'
                'with open(os.environ["GIT_CAPTURE"],"a") as log: log.write(json.dumps(sys.argv[1:])+"\\n")\n'
                'os.execv(os.environ["REAL_GIT"],[os.environ["REAL_GIT"],*sys.argv[1:]])\n')
            (tools / 'git').chmod(0o755)
            (tools / 'gh').write_text(
                '#!/usr/bin/env python3\nimport json,os,sys\n'
                'if sys.argv[1:3] == ["api","user"]: print("synthetic")\n'
                'else: open(os.environ["GH_CAPTURE"],"w").write(json.dumps(sys.argv[1:]))\n')
            (tools / 'gh').chmod(0o755)
            images = root / 'images'
            images.mkdir()
            image = images / 'control.png'
            image.write_bytes(b'synthetic first image')
            body = root / 'body.md'
            body.write_text('### Session control\n\n{{control}}\n')

            def publish():
                return subprocess.run(['bash', str(SCRIPT), '--repo', 'example/buzz-fork',
                                       '--remote', 'fork', '15', str(images), str(body)],
                                      cwd=checkout, env=env, capture_output=True, text=True)

            first = publish()
            self.assertEqual(first.returncode, 0, first.stderr)
            ref = 'refs/heads/agent-screenshots/synthetic'
            initial = git('rev-parse', ref, cwd=remote)
            image.write_bytes(b'synthetic replacement image')
            second = publish()
            self.assertEqual(second.returncode, 0, second.stderr)
            final = git('rev-parse', ref, cwd=remote)
            self.assertNotEqual(initial, final)
            self.assertEqual(git('rev-parse', final + '^', cwd=remote), initial)
            self.assertEqual(git('show', final + ':pr-15--control.png', cwd=remote),
                             'synthetic replacement image')
            self.assertIn('Signed-off-by: Synthetic Contributor <contributor@example.invalid>',
                          git('show', '-s', '--format=%B', final, cwd=remote))
            comment = json.loads((root / 'gh.json').read_text())
            self.assertEqual(comment[:5], ['pr', 'comment', '15', '--repo', 'example/buzz-fork'])
            self.assertIn(f'https://raw.githubusercontent.com/example/buzz-fork/{final}/pr-15--control.png',
                          comment[-1])
            self.assertNotIn('{{control}}', comment[-1])
            pushes = [json.loads(line) for line in (root / 'git.jsonl').read_text().splitlines()
                      if json.loads(line)[0] == 'push']
            self.assertEqual(pushes, [['push', 'fork', f'{initial}:{ref}'],
                                      ['push', 'fork', f'{final}:{ref}']])
            # Publication is ordered: a rejected Git push cannot post broken links.
            hook = remote / 'hooks/pre-receive'
            hook.write_text('#!/bin/sh\nexit 1\n')
            hook.chmod(0o755)
            (root / 'gh.json').unlink()
            image.write_bytes(b'rejected image')
            rejected = publish()
            self.assertNotEqual(rejected.returncode, 0)
            self.assertEqual(git('rev-parse', ref, cwd=remote), final)
            self.assertFalse((root / 'gh.json').exists())


if __name__ == '__main__':
    unittest.main()
