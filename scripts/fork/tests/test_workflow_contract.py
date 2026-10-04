"""Inspect and exercise the actual JSON-form YAML Actions job/step graph."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


WORKFLOW = Path(__file__).resolve().parents[3] / '.github/workflows/fork-sync.yml'
PYTHON_GATE = 'python3 -m unittest discover -s scripts/fork/tests -v'


class WorkflowContractTests(unittest.TestCase):
    def workflow(self):
        self.assertTrue(WORKFLOW.is_file(), 'guarded sync workflow is missing')
        return json.loads(WORKFLOW.read_text())

    def test_every_candidate_workflow_runs_fork_python_suite(self):
        workflow = self.workflow()
        jobs = workflow['jobs']
        candidate = jobs['candidate']
        self.assertIn('baseline', candidate['needs'])
        self.assertEqual(candidate['if'], "${{ needs.baseline.result == 'success' && needs.select.outputs.target_sha != '' }}")
        commands = [s.get('run', '') for s in candidate['steps']]
        self.assertIn(PYTHON_GATE, commands)
        gate = commands.index(PYTHON_GATE)
        build = next(i for i, c in enumerate(commands) if 'build-candidate.sh' in c)
        self.assertLess(gate, build)
        self.assertTrue(any('just ci' in c for c in commands[gate + 1:build]))
        for step in candidate['steps']:
            self.assertFalse(step.get('continue-on-error', False))
            if step.get('id') != 'report_upload':
                self.assertNotIn('always()', step.get('if', ''))
        baseline_commands = '\n'.join(s.get('run', '') for s in jobs['baseline']['steps'])
        self.assertIn('baseline', baseline_commands)
        baseline_step = next(s for s in jobs['baseline']['steps'] if ' sync.py baseline ' in s.get('run', '')
                             or 'scripts/fork/sync.py baseline ' in s.get('run', ''))
        self.assertEqual(baseline_step['env']['TARGET_SHA'], '${{ needs.select.outputs.target_sha }}')
        self.assertEqual(workflow['concurrency']['cancel-in-progress'], False)
        self.assertEqual(workflow['on']['schedule'], [{'cron': '0 6 * * *'}])
        self.assertIn('workflow_dispatch', workflow['on'])
        self.assertEqual(set(workflow['permissions']), {'contents', 'pull-requests'})

    def test_candidate_uses_exact_m3_producer_and_verified_output(self):
        steps = self.workflow()['jobs']['candidate']['steps']
        build = next(step for step in steps if step.get('name') == 'M3 real candidate build and verification')
        self.assertEqual(build['env']['CANDIDATE_SHA'], '${{ steps.prepare.outputs.candidate_sha }}')
        self.assertIn('scripts/fork/build-candidate.sh --candidate-sha "$CANDIDATE_SHA" --baseline-report "$GITHUB_WORKSPACE/baseline.json"', build['run'])
        self.assertIn('unset BUZZ_UPDATER_ENDPOINT BUZZ_UPDATER_PUBLIC_KEY', build['run'])
        upload = next(step for step in steps if step.get('name') == 'Upload verified candidate artifacts')
        self.assertEqual(upload['with']['path'], '${{ steps.prepare.outputs.worktree }}/artifacts/fork/*')
        self.assertEqual(upload['with']['if-no-files-found'], 'error')

    def test_failed_clean_target_never_executes_candidate_steps(self):
        # Execute an injected baseline failure and evaluate the actual job guard.
        # This local graph fixture exercises ordering, not GitHub runner acceptance.
        workflow = self.workflow()
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / 'candidate-ran'
            baseline = subprocess.run(['python3', '-c', 'raise SystemExit(7)'])
            result = 'success' if baseline.returncode == 0 else 'failure'
            candidate = workflow['jobs']['candidate']
            guard = candidate.get('if', '')
            self.assertIn('baseline', candidate['needs'])
            self.assertEqual(guard, "${{ needs.baseline.result == 'success' && needs.select.outputs.target_sha != '' }}")
            expression = guard.removeprefix('${{').removesuffix('}}').strip()
            expression = expression.replace('needs.baseline.result', repr(result))
            expression = expression.replace('needs.select.outputs.target_sha', repr('synthetic-target-sha'))
            expression = expression.replace('&&', 'and')
            admitted = eval(expression, {'__builtins__': {}}, {})
            if admitted:
                subprocess.run(['python3', '-c', 'import pathlib,sys; pathlib.Path(sys.argv[1]).touch()',
                                str(marker)], check=True)
            self.assertFalse(marker.exists(), 'candidate tests/build/upload must not begin')
        upload_jobs = [job for name, job in workflow['jobs'].items()
                       if name not in ('select', 'baseline', 'candidate', 'report')]
        self.assertEqual(upload_jobs, [])
        self.assertNotIn('continue-on-error', json.dumps(workflow))


if __name__ == '__main__':
    unittest.main()
