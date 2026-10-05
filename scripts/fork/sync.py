#!/usr/bin/env python3
"""Validate the fork patch inventory against its upstream base and live tree."""
import argparse
from contextlib import contextmanager
from dataclasses import asdict, dataclass, field
import copy
import fcntl
import hashlib
import json
import os
import plistlib
from pathlib import Path
import re
import subprocess
import tempfile


IDENTITY_WRAPPER = Path.home() / '.local/bin/git'
GIT = str(IDENTITY_WRAPPER) if IDENTITY_WRAPPER.is_file() else 'git'
UPSTREAM_URL = 'https://github.com/block/buzz.git'
REPOSITORY_URL = 'https://github.com/reynholm/buzz'
PYTHON_GATE = 'python3 -m unittest discover -s scripts/fork/tests -v'
BASELINE_GATES = [
    ['pnpm', 'install', '--frozen-lockfile'], ['just', 'mobile-install'],
    ['just', 'fmt-check', 'clippy', 'test-unit', 'desktop-check', 'desktop-typecheck',
     'desktop-tauri-fmt-check', 'desktop-tauri-test', 'desktop-test'],
    ['pnpm', '--dir', 'desktop', 'lint'], ['pnpm', '--dir', 'desktop', 'build'],
    ['just', 'ci'],
    ['cargo', 'build', '--release', '--target', 'aarch64-apple-darwin', '-p', 'buzz-acp',
     '-p', 'buzz-agent', '-p', 'buzz-backend-kubernetes', '-p', 'buzz-dev-mcp',
     '-p', 'git-credential-nostr', '-p', 'buzz-cli'],
    ['./scripts/bundle-sidecars.sh', 'aarch64-apple-darwin'],
    ['just', 'desktop-release-build', 'aarch64-apple-darwin'],
]


@dataclass
class UpdateSelection:
    """Newest stable desktop tag plus the intermediate releases skipped."""
    target_tag: str
    skipped_tags: list[str]


@dataclass
class SyncReport:
    """Public, path-free evidence for one local release preparation."""
    state: str
    base_sha: str | None = None
    target_sha: str | None = None
    candidate_sha: str | None = None
    branch: str | None = None
    target_tag: str | None = None
    skipped_tags: list[str] = field(default_factory=list)
    conflicts: list[str] = field(default_factory=list)
    missing_seams: list[str] = field(default_factory=list)
    prior_base: dict = field(default_factory=dict)
    clean_target: dict = field(default_factory=dict)
    reason: str = ''
    report_branch: str | None = None
    report_sha: str | None = None
    pr_url: str | None = None
    artifact_url: str | None = None
    repository_url: str = REPOSITORY_URL


def git(repo: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess:
    """Run Git with argv boundaries and the local identity wrapper when present."""
    return subprocess.run([GIT, '-C', str(repo), *args], check=check,
                          capture_output=True, text=True)


def select_update(tags: list[str], current_tag: str) -> UpdateSelection | None:
    """Select stable desktop semver releases; prerelease/mobile tags are ignored."""
    def version(tag):
        match = re.fullmatch(r'desktop-v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)', tag)
        return tuple(map(int, match.groups())) if match else None
    current = version(current_tag)
    if current is None:
        raise ValueError('current tag must be a stable desktop release')
    newer = sorted({tag for tag in tags if version(tag) and version(tag) > current}, key=version)
    return UpdateSelection(newer[-1], newer[:-1]) if newer else None


def read_upstream_tags(repo: Path, url: str = UPSTREAM_URL) -> list[str]:
    """Read public release refs without creating remotes or moving local refs."""
    output = git(repo, 'ls-remote', '--tags', '--refs', url, 'desktop-v*').stdout
    return [line.split('\trefs/tags/', 1)[1] for line in output.splitlines()]


def state_directory(repo: Path) -> Path:
    """Return private preparation state shared by all worktrees of this repository."""
    common = Path(git(repo, 'rev-parse', '--git-common-dir').stdout.strip())
    return (common if common.is_absolute() else repo / common).resolve() / 'fork-sync'


@contextmanager
def preparation_lock(repo: Path):
    """Serialize preparation and report writes with an OS lock, including after crashes."""
    directory = state_directory(repo)
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / 'lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            yield directory
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def write_json(path: Path, value: dict):
    """Atomically replace an owned JSON journal/report."""
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    temporary.replace(path)


def commit_owned(repo: Path, paths: list[str], message: str):
    """Stage explicit paths and DCO-sign a local tooling commit with configured identity."""
    name = git(repo, 'config', 'user.name', check=False).stdout.strip()
    email = git(repo, 'config', 'user.email', check=False).stdout.strip()
    signing = git(repo, 'config', 'commit.gpgsign', check=False).stdout.strip()
    if not name or not email or signing == 'true':
        raise RuntimeError('configured author/email and unsigned commit policy required')
    if IDENTITY_WRAPPER.is_file():
        global_name = git(repo, 'config', '--global', 'user.name').stdout.strip()
        global_email = git(repo, 'config', '--global', 'user.email').stdout.strip()
        if (name, email) != (global_name, global_email) or signing:
            raise RuntimeError('trusted global identity guard failed')
    if paths:
        git(repo, 'add', '--', *paths)
    git(repo, 'commit', '-s', '-m', message)


def merge_target(worktree: Path, target_sha: str):
    """Merge without rebasing; leave an unresolved index in this owned worktree."""
    return git(worktree, 'merge', '--no-ff', '--no-commit', target_sha, check=False)


def prepare_update(repo: Path, selection: UpdateSelection | None, manifest: dict,
                   *, base: str = 'main', clean_target: dict | None = None) -> SyncReport:
    """Prepare one local merge only after the exact clean target baseline succeeds."""
    if selection is None:
        return SyncReport('no_update')
    report = SyncReport('blocked', target_tag=selection.target_tag,
                        skipped_tags=selection.skipped_tags,
                        prior_base={k: manifest[k] for k in ('base_tag', 'base_sha')},
                        clean_target=clean_target or {})
    if not re.fullmatch(r'desktop-v\d+\.\d+\.\d+', selection.target_tag):
        report.reason = 'invalid release tag'
        return report
    report.branch = 'fork/sync-' + selection.target_tag.removeprefix('desktop-')
    target = git(repo, 'rev-parse', '--verify', f'refs/tags/{selection.target_tag}^{{commit}}', check=False)
    accepted = git(repo, 'rev-parse', '--verify', f'{base}^{{commit}}', check=False)
    if target.returncode or accepted.returncode:
        report.reason = 'selected tag or accepted base ref unavailable'
        return report
    report.target_sha, report.base_sha = target.stdout.strip(), accepted.stdout.strip()
    # This guard must precede mirror, worktree, branch, merge and candidate operations.
    if (not clean_target or clean_target.get('result') != 'success'
            or clean_target.get('upstream_sha') != report.target_sha
            or not clean_target.get('gate_commands') or not clean_target.get('artifact_manifest')):
        report.reason = 'exact clean target baseline has not succeeded'
        return report
    with preparation_lock(repo) as directory:
        journal = directory / f'{selection.target_tag}.json'
        worktree = directory / 'worktrees' / selection.target_tag
        existing = git(repo, 'rev-parse', '--verify', f'refs/heads/{report.branch}', check=False)
        owned = json.loads(journal.read_text()) if journal.exists() else None
        if existing.returncode == 0 and not owned:
            report.reason = 'branch ownership conflict'
            return report
        if owned and (owned['base_sha'] != report.base_sha or owned['target_sha'] != report.target_sha):
            report.reason = 'recorded base moved or target changed; owner handoff required'
            return report
        if owned and owned.get('candidate_sha'):
            if existing.stdout.strip() != owned['candidate_sha']:
                report.reason = 'candidate ownership conflict'
                return report
            if not worktree.exists():
                git(repo, 'worktree', 'add', str(worktree), report.branch)
            candidate_manifest = json.loads((worktree / 'scripts/fork/patches.json').read_text())
            if (git(worktree, 'rev-parse', 'HEAD').stdout.strip() != owned['candidate_sha']
                    or candidate_manifest.get('base_sha') != report.target_sha
                    or git(worktree, 'status', '--porcelain').stdout
                    or validate_patches(worktree, candidate_manifest)):
                report.reason = 'reused candidate worktree changed or lost protected seams'
                return report
            report.candidate_sha = owned['candidate_sha']
            report.state = 'ready'
            return report
        # The mirror is never rebased or rewound. Accepted main is never moved.
        mirror = git(repo, 'rev-parse', '--verify', 'refs/heads/upstream', check=False)
        if 'branch refs/heads/upstream\n' in git(repo, 'worktree', 'list', '--porcelain').stdout:
            report.reason = 'upstream mirror is checked out in a foreign worktree'
            return report
        if mirror.returncode == 0:
            if git(repo, 'merge-base', '--is-ancestor', mirror.stdout.strip(), report.target_sha,
                   check=False).returncode:
                report.reason = 'upstream mirror cannot fast-forward'
                return report
            git(repo, 'update-ref', 'refs/heads/upstream', report.target_sha, mirror.stdout.strip())
        else:
            git(repo, 'update-ref', 'refs/heads/upstream', report.target_sha, '0' * 40)
        write_json(journal, asdict(report))
        if not worktree.exists():
            if existing.returncode == 0:
                if existing.stdout.strip() != report.base_sha:
                    report.reason = 'interrupted branch ownership conflict'
                    return report
                git(repo, 'worktree', 'add', str(worktree), report.branch)
            else:
                git(repo, 'worktree', 'add', '-b', report.branch, str(worktree), report.base_sha)
        head = git(worktree, 'rev-parse', 'HEAD').stdout.strip()
        if head != report.base_sha:
            report.reason = 'worktree ownership conflict'
            return report
        unmerged = git(worktree, 'diff', '--name-only', '--diff-filter=U').stdout.splitlines()
        merging = git(worktree, 'rev-parse', '--verify', 'MERGE_HEAD', check=False)
        if merging.returncode and git(worktree, 'status', '--porcelain').stdout:
            report.reason = 'worktree contains unowned changes'
            return report
        if merging.returncode:
            errors = validate_patches(worktree, manifest)
            if errors:
                report.missing_seams = errors
                report.reason = 'accepted base registry is invalid'
                return report
            merge = merge_target(worktree, report.target_sha)
            unmerged = git(worktree, 'diff', '--name-only', '--diff-filter=U').stdout.splitlines()
            if merge.returncode and not unmerged:
                raise RuntimeError('merge failed without a conflicted index')
        protected = {p['path'] for p in manifest['patches']}
        protected.update(s['caller_path'] for p in manifest['patches'] for s in p['invocation_seams'])
        for path in unmerged:
            if path not in protected:
                # Only this previously validated inventory permits upstream resolution.
                theirs = git(worktree, 'show', f':3:{path}', check=False)
                if theirs.returncode:
                    git(worktree, 'rm', '--', path)
                else:
                    git(worktree, 'checkout', '--theirs', '--', path)
                    git(worktree, 'add', '--', path)
        report.conflicts = git(worktree, 'diff', '--name-only', '--diff-filter=U').stdout.splitlines()
        if report.conflicts:
            report.reason = 'registered patch conflicts require repair and owner handoff'
            write_json(journal, asdict(report))
            return report
        candidate_manifest = copy.deepcopy(manifest)
        candidate_manifest.update(base_tag=selection.target_tag, base_sha=report.target_sha)
        target_paths = set(git(repo, 'ls-tree', '-r', '--name-only', report.target_sha).stdout.splitlines())
        for item in candidate_manifest['patches']:
            item['is_new'] = item['path'] not in target_paths
        write_json(worktree / 'scripts/fork/patches.json', candidate_manifest)
        (worktree / 'FORK_PATCHES.md').write_text(render_registry(candidate_manifest))
        report.missing_seams = validate_patches(worktree, candidate_manifest)
        if report.missing_seams:
            report.reason = 'candidate patch validation failed'
            write_json(journal, asdict(report))
            return report
        if git(worktree, 'diff', '--check', report.target_sha, check=False).returncode:
            report.reason = 'candidate contains conflict markers or whitespace errors'
            write_json(journal, asdict(report))
            return report
        commit_owned(worktree, ['scripts/fork/patches.json', 'FORK_PATCHES.md'],
                     f'feat(fork): merge {selection.target_tag} with protected patches\n\n'
                     f'Fork-Sync-Base: {report.base_sha}\nFork-Sync-Target: {report.target_sha}')
        report.candidate_sha = git(worktree, 'rev-parse', 'HEAD').stdout.strip()
        report.state = 'ready'
        write_json(journal, asdict(report))
        return report


def reuse_published_candidate(repo: Path, selection: UpdateSelection, manifest: dict,
                              *, base='main', clean_target: dict | None = None) -> SyncReport | None:
    """Hydrate an ephemeral runner from a verified, previously published merge commit."""
    branch = 'fork/sync-' + selection.target_tag.removeprefix('desktop-')
    remote = git(repo, 'ls-remote', '--refs', 'origin', f'refs/heads/{branch}').stdout.strip()
    if not remote:
        return None
    target = git(repo, 'rev-parse', f'refs/tags/{selection.target_tag}^{{commit}}').stdout.strip()
    accepted = git(repo, 'rev-parse', base).stdout.strip()
    report = SyncReport('blocked', base_sha=accepted, target_sha=target, branch=branch,
                        target_tag=selection.target_tag, skipped_tags=selection.skipped_tags,
                        prior_base={k: manifest[k] for k in ('base_tag', 'base_sha')},
                        clean_target=clean_target or {})
    if (not clean_target or clean_target.get('result') != 'success'
            or clean_target.get('upstream_sha') != target
            or not clean_target.get('gate_commands') or not clean_target.get('artifact_manifest')):
        report.reason = 'exact clean target baseline has not succeeded'
        return report
    with preparation_lock(repo) as directory:
        git(repo, 'fetch', '--no-tags', 'origin', f'refs/heads/{branch}')
        sha = git(repo, 'rev-parse', 'FETCH_HEAD').stdout.strip()
        parents = git(repo, 'rev-list', '--parents', '-n', '1', sha).stdout.split()[1:]
        message = git(repo, 'show', '-s', '--format=%B', sha).stdout
        if (parents != [accepted, target] or f'Fork-Sync-Base: {accepted}' not in message
                or f'Fork-Sync-Target: {target}' not in message):
            report.reason = 'published branch ownership conflict or base moved'
            return report
        existing = git(repo, 'rev-parse', '--verify', f'refs/heads/{branch}', check=False)
        journal = directory / f'{selection.target_tag}.json'
        if existing.returncode == 0 and (not journal.exists() or existing.stdout.strip() != sha):
            report.reason = 'local branch ownership conflict'
            return report
        worktree = directory / 'worktrees' / selection.target_tag
        if not worktree.exists():
            if existing.returncode:
                git(repo, 'update-ref', f'refs/heads/{branch}', sha, '0' * 40)
            git(repo, 'worktree', 'add', str(worktree), branch)
        candidate_manifest = json.loads((worktree / 'scripts/fork/patches.json').read_text())
        expected_manifest = copy.deepcopy(manifest)
        expected_manifest.update(base_sha=target, base_tag=selection.target_tag)
        paths = set(git(repo, 'ls-tree', '-r', '--name-only', target).stdout.splitlines())
        for item in expected_manifest['patches']:
            item['is_new'] = item['path'] not in paths
        report.missing_seams = validate_patches(worktree, candidate_manifest)
        if (candidate_manifest != expected_manifest or report.missing_seams
                or git(worktree, 'status', '--porcelain').stdout
                or git(worktree, 'rev-parse', 'HEAD').stdout.strip() != sha):
            report.reason = 'published candidate does not preserve the accepted registry'
            return report
        report.state, report.candidate_sha = 'ready', sha
        write_json(journal, asdict(report))
        return report


def reuse_published_report(repo: Path, selection: UpdateSelection, report: SyncReport):
    """Reuse a remote report-only commit after validating its compare diff and target."""
    branch = 'fork/blocked-' + selection.target_tag.removeprefix('desktop-')
    path = f'docs/fork/sync-reports/{selection.target_tag}.json'
    if not git(repo, 'ls-remote', '--refs', 'origin', f'refs/heads/{branch}').stdout.strip():
        return
    with preparation_lock(repo) as directory:
        git(repo, 'fetch', '--no-tags', 'origin', f'refs/heads/{branch}')
        sha = git(repo, 'rev-parse', 'FETCH_HEAD').stdout.strip()
        parents = git(repo, 'rev-list', '--parents', '-n', '1', sha).stdout.split()[1:]
        changes = git(repo, 'diff', '--name-only', report.base_sha, sha).stdout.splitlines()
        document = json.loads(git(repo, 'show', f'{sha}:{path}').stdout)
        if (parents != [report.base_sha] or changes != [path] or document['state'] != 'blocked'
                or document['base_sha'] != report.base_sha or document['target_sha'] != report.target_sha):
            raise RuntimeError('published blocked report ownership conflict')
        existing = git(repo, 'rev-parse', '--verify', f'refs/heads/{branch}', check=False)
        journal = directory / f'blocked-{selection.target_tag}.json'
        if existing.returncode == 0 and (not journal.exists() or existing.stdout.strip() != sha):
            raise RuntimeError('local blocked branch ownership conflict')
        if existing.returncode:
            git(repo, 'update-ref', f'refs/heads/{branch}', sha, '0' * 40)
        report.report_branch, report.report_sha = branch, sha
        write_json(journal, asdict(report))


def prepare_blocked_report(repo: Path, selection: UpdateSelection, report: SyncReport,
                           *, base: str = 'main') -> str:
    """Create/reuse a clean report-only branch; never stage the conflicted worktree."""
    if report.state != 'blocked' or not report.base_sha:
        raise ValueError('a blocked report with an accepted base is required')
    branch = 'fork/blocked-' + selection.target_tag.removeprefix('desktop-')
    path = f'docs/fork/sync-reports/{selection.target_tag}.json'
    with preparation_lock(repo) as directory:
        if git(repo, 'rev-parse', base).stdout.strip() != report.base_sha:
            raise RuntimeError('accepted base moved before blocked report')
        journal = directory / f'blocked-{selection.target_tag}.json'
        existing = git(repo, 'rev-parse', '--verify', f'refs/heads/{branch}', check=False)
        if existing.returncode == 0:
            owned = json.loads(journal.read_text()) if journal.exists() else None
            if not owned or owned['base_sha'] != report.base_sha or owned['target_sha'] != report.target_sha:
                raise RuntimeError('blocked branch ownership conflict')
            if existing.stdout.strip() != owned.get('report_sha'):
                raise RuntimeError('interrupted blocked branch requires owner handoff')
            report.report_branch, report.report_sha = branch, owned['report_sha']
            return report.report_sha
        worktree = directory / 'reports' / selection.target_tag
        if worktree.exists():
            raise RuntimeError('report worktree ownership conflict')
        write_json(journal, asdict(report))
        git(repo, 'worktree', 'add', '-b', branch, str(worktree), report.base_sha)
        document = asdict(report)
        # Report contains only refs, relative registered paths and public evidence.
        document['clean_target'] = {k: report.clean_target.get(k) for k in ('upstream_sha', 'result')}
        write_json(worktree / path, document)
        commit_owned(worktree, [path], f'docs(fork): report blocked {selection.target_tag}')
        report.report_branch = branch
        report.report_sha = git(worktree, 'rev-parse', 'HEAD').stdout.strip()
        write_json(journal, asdict(report))
        return report.report_sha


def push_branch(repo: Path, branch: str):
    """Publish one clean branch without force or implicit other refs."""
    git(repo, 'push', 'origin', f'refs/heads/{branch}:refs/heads/{branch}')


def gh_command(argv: list[str]) -> str:
    """Run the GitHub CLI using structured argv, propagating API failures."""
    return subprocess.run(argv, check=True, capture_output=True, text=True).stdout.strip()


def draft_body(report: SyncReport) -> str:
    """Make candidate provenance and pending owner checks reviewable in the draft."""
    prior_tag = report.prior_base.get('base_tag', 'unknown')
    return ('Fork update: ' + prior_tag + ' → ' + str(report.target_tag)
            + '\nUpstream changelog: https://github.com/block/buzz/releases/tag/' + str(report.target_tag)
            + '\nUpstream range: https://github.com/block/buzz/compare/' + prior_tag + '...' + str(report.target_tag)
            + '\nReview persona, agent, device ownership and sync changes in the compare diff.'
            + '\nCheck conflicts and protected invocation seams in the report below.'
            + '\nRequired full tests and clean baseline receipts must match the exact candidate SHA.'
            + '\nVerified manifest, baseline.json and SHA256SUMS accompany candidate artifacts.'
            + '\nOwner acceptance: native About SHA, existing data, sidecar runtime, keychain prompt,'
            + ' two physical devices and maintenance command; follow docs/fork/MAINTENANCE.md.'
            + '\nKeep draft until owner review and acceptance; owner merges before exact-tree promotion.'
            + '\n\n```json\n' + json.dumps(asdict(report), indent=2) + '\n```\n')


def gh_body(argv: list[str], report: SyncReport, gh):
    """Pass exact multiline PR evidence via a private temporary file."""
    with tempfile.NamedTemporaryFile(mode='w', suffix='.md') as body:
        body.write(draft_body(report))
        body.flush()
        return gh([*argv, '--body-file', body.name])


def publish_draft(repo: Path, report: SyncReport, *, gh=gh_command, base='main') -> str:
    """Explicit opt-in publication: reuse one draft PR and never promote or merge."""
    branch = report.report_branch if report.state == 'blocked' else report.branch
    sha = report.report_sha if report.state == 'blocked' else report.candidate_sha
    if not branch or not sha or report.state not in ('ready', 'blocked'):
        raise ValueError('only clean candidate/report commits can be published')
    if git(repo, 'rev-parse', branch).stdout.strip() != sha:
        raise RuntimeError('publication branch SHA changed')
    with preparation_lock(repo):
        push_branch(repo, branch)
        existing = json.loads(gh(['gh', 'pr', 'list', '--repo', 'reynholm/buzz', '--head', branch,
                                  '--base', base, '--state', 'all', '--json', 'url,isDraft,state']))
        if existing:
            if existing[0].get('state', 'OPEN') != 'OPEN' or not existing[0].get('isDraft', True):
                raise RuntimeError('existing PR is closed or promoted; owner handoff required')
            report.pr_url = existing[0]['url']
        else:
            report.pr_url = gh_body(['gh', 'pr', 'create', '--repo', 'reynholm/buzz', '--head', branch,
                                '--base', base, '--draft', '--title',
                                f'{report.state}: {report.target_tag}'], report, gh)
        return report.pr_url


def run_clean_target(repo: Path, target_sha: str, report_path: Path) -> dict:
    """Execute real upstream gates/build in a separate exact detached target worktree."""
    evidence = {'upstream_sha': target_sha, 'gate_commands': [], 'result': 'failure',
                'artifact_manifest': {}}
    report_path = report_path.resolve()
    with preparation_lock(repo) as directory:
        worktree = directory / 'baseline' / target_sha
        if worktree.exists():
            raise RuntimeError('existing baseline worktree requires owner handoff')
        git(repo, 'worktree', 'add', '--detach', str(worktree), target_sha)
        env = {k: v for k, v in os.environ.items()
               if not k.startswith('GIT_CONFIG_') and k != 'BUZZ_ACP_SESSION_POLICY'
               and not k.startswith('BUZZ_FORK_')}
        env['CI'] = 'true'
        # Keep Hermit activation inside the exact upstream checkout, never the fork.
        env['PATH'] = str(worktree / 'bin') + os.pathsep + env.get('PATH', '')
        try:
            for number, command in enumerate(BASELINE_GATES):
                evidence['gate_commands'].append(command)
                write_json(report_path, evidence)
                with report_path.with_suffix(f'.gate-{number}.log').open('w') as log:
                    log.write(f'upstream_sha={target_sha}\ncommand={json.dumps(command)}\n')
                    log.flush()
                    subprocess.run(command, cwd=worktree, env=env, stdout=log,
                                   stderr=subprocess.STDOUT, check=True)
            bundle = worktree / 'desktop/src-tauri/target/aarch64-apple-darwin/release/bundle'
            app = next(bundle.glob('macos/*.app'))
            binaries = app / 'Contents/MacOS'
            names = ['buzz', 'buzz-acp', 'buzz-agent', 'buzz-backend-kubernetes',
                     'buzz-dev-mcp', 'git-credential-nostr']
            with (app / 'Contents/Info.plist').open('rb') as stream:
                executable = plistlib.load(stream)['CFBundleExecutable']
            files = [binaries / executable, *[binaries / name for name in names],
                     next(bundle.glob('dmg/*.dmg'))]
            artifacts = {}
            for artifact in files:
                if not artifact.is_file() or not artifact.stat().st_size:
                    raise RuntimeError('missing real baseline artifact')
                if artifact.parent == binaries:
                    description = subprocess.check_output(['file', '-b', str(artifact)], text=True)
                    if not os.access(artifact, os.X_OK) or 'Mach-O' not in description or 'arm64' not in description:
                        raise RuntimeError('baseline binary is not executable Mach-O arm64')
                with artifact.open('rb') as stream:
                    digest = hashlib.file_digest(stream, 'sha256').hexdigest()
                artifacts[str(artifact.relative_to(bundle))] = {'sha256': digest,
                                                              'size': artifact.stat().st_size}
            if git(worktree, 'rev-parse', 'HEAD').stdout.strip() != target_sha or git(worktree, 'status', '--porcelain').stdout:
                raise RuntimeError('clean target changed during baseline')
            evidence['artifact_manifest'] = artifacts
            evidence['result'] = 'success'
        finally:
            write_json(report_path, evidence)
    return evidence


def render_registry(manifest: dict) -> str:
    """Render the authoritative JSON inventory as deterministic Markdown."""
    lines = ['# Fork patches', '', 'Generated from `scripts/fork/patches.json`; regenerate with',
             '`python3 scripts/fork/sync.py render`.', '',
             f"Base: `{manifest['base_tag']}` (`{manifest['base_sha']}`).", '']
    for patch in manifest['patches']:
        lines.extend([f"## `{patch['path']}`", '', patch['responsibility'], '',
                      f"New module: `{str(patch['is_new']).lower()}`.", ''])
        if patch.get('deleted') is True:
            lines.extend(['Registered upstream deletion: `true`.', ''])
        for symbol in patch['required_symbols']:
            lines.append(f'- Required symbol: `{symbol}`')
        for seam in patch['invocation_seams']:
            lines.append(f"- Invocation: `{seam['caller_path']}` → `{seam['callee_symbol']}`; "
                         f"exact call `{seam['invocation']}`; behavior test "
                         f"`{seam['behavior_test']}`; verify `{seam['verification_command']}`")
        for command in patch['verification_commands']:
            lines.append(f'- Verify: `{command}`')
        lines.append('')
    return '\n'.join(lines)


def validate_patches(repo: Path, manifest: dict) -> list[str]:
    """Return lost paths/symbols/calls and inventory drift; an empty list is valid."""
    errors = []
    try:
        result = subprocess.run(['git', '-C', str(repo), 'diff', '--name-only', '-z',
                                 manifest['base_sha'], '--'], check=True, capture_output=True)
        changed = set(result.stdout.decode().split('\0')) - {''}
        base = subprocess.run(['git', '-C', str(repo), 'ls-tree', '-r', '--name-only', '-z',
                               manifest['base_sha']], check=True, capture_output=True)
        upstream = set(base.stdout.decode().split('\0')) - {''}
    except subprocess.CalledProcessError as exc:
        return [f'cannot inspect base: {exc.stderr.decode().strip()}']
    patches = manifest['patches']
    paths = [patch['path'] for patch in patches]
    if len(paths) != len(set(paths)):
        errors.append('duplicate registry path')
    errors.extend(f'unlisted path: {path}' for path in sorted(changed - set(paths)))
    for patch in patches:
        path = patch['path']
        target = repo / path
        if Path(path).is_absolute() or '..' in Path(path).parts:
            errors.append(f'invalid path: {path}')
            continue
        if patch['is_new'] != (path not in upstream):
            errors.append(f'wrong is_new: {path}')
        if patch.get('deleted'):
            if (patch['deleted'] is not True or path not in upstream
                    or patch['required_symbols'] or patch['invocation_seams']):
                errors.append(f'invalid registered deletion: {path}')
            elif target.exists() or target.is_symlink():
                errors.append(f'registered deletion still exists: {path}')
            continue
        if not target.is_file():
            errors.append(f'missing path: {path}')
            continue
        content = target.read_text() if patch['required_symbols'] else ''
        for symbol in patch['required_symbols']:
            if symbol not in content:
                errors.append(f'missing symbol: {path}: {symbol}')
        for seam in patch['invocation_seams']:
            caller_path = seam['caller_path']
            caller = repo / caller_path
            if Path(caller_path).is_absolute() or '..' in Path(caller_path).parts:
                errors.append(f'invalid caller path: {caller_path}')
                continue
            # Exact call plus symbol token prevents a renamed callee passing on
            # an unrelated substring. Behavior commands are run separately.
            if (not caller.is_file() or seam['invocation'] not in caller.read_text()
                    or not re.search(r'(?<![\w])' + re.escape(seam['callee_symbol'])
                                     + r'(?![\w])', seam['invocation'])
                    or not seam.get('behavior_test') or not seam.get('verification_command')):
                errors.append(f"missing invocation: {caller_path}: {seam['callee_symbol']}")
    markdown = repo / 'FORK_PATCHES.md'
    if not markdown.is_file() or markdown.read_text() != render_registry(manifest):
        errors.append('Markdown registry differs')
    return errors


def main() -> int:
    """Run validation or explicitly regenerate the Markdown inventory."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['validate', 'render', 'select', 'baseline', 'prepare', 'publish', 'link-artifact'],
                        nargs='?', default='validate')
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--base', default='main')
    parser.add_argument('--manifest', type=Path, default=Path('scripts/fork/patches.json'))
    parser.add_argument('--report', type=Path)
    parser.add_argument('--baseline-report', type=Path)
    parser.add_argument('--target-sha')
    parser.add_argument('--artifact-url')
    parser.add_argument('--offline', action='store_true', help='use already fetched tags (local fixtures)')
    parser.add_argument('--publish', action='store_true', help='explicitly authorize branch/draft PR publication')
    args = parser.parse_args()
    args.repo = args.repo.resolve()
    if args.command in ('select', 'baseline', 'prepare', 'publish', 'link-artifact') and not args.report:
        parser.error('--report is required')
    if args.command == 'baseline':
        if not args.target_sha or not re.fullmatch(r'[0-9a-f]{40}', args.target_sha):
            parser.error('--target-sha must be an exact commit SHA')
        evidence = run_clean_target(args.repo, args.target_sha, args.report)
        return evidence['result'] != 'success'
    if args.command == 'publish':
        if not args.publish:
            parser.error('publication requires --publish')
        report = SyncReport(**json.loads(args.report.read_text()))
        publish_draft(args.repo, report, base=args.base)
        write_json(args.report, asdict(report))
        return 0
    if args.command == 'link-artifact':
        if not args.publish or not args.artifact_url or not args.artifact_url.startswith(REPOSITORY_URL + '/actions/runs/'):
            parser.error('artifact linkage requires --publish and this repository Actions URL')
        report = SyncReport(**json.loads(args.report.read_text()))
        if report.state != 'ready' or not report.pr_url or not report.pr_url.startswith(REPOSITORY_URL + '/pull/'):
            parser.error('a ready candidate with its existing draft PR is required')
        report.artifact_url = args.artifact_url
        gh_body(['gh', 'pr', 'edit', report.pr_url], report, gh_command)
        write_json(args.report, asdict(report))
        return 0
    manifest = json.loads((args.repo / args.manifest).read_text())
    if args.command in ('select', 'prepare'):
        tags = (git(args.repo, 'tag', '--list', 'desktop-v*').stdout.splitlines()
                if args.offline else read_upstream_tags(args.repo))
        selection = select_update(tags, manifest['base_tag'])
        if selection and not args.offline:
            git(args.repo, 'fetch', '--no-tags', UPSTREAM_URL,
                f'refs/tags/{selection.target_tag}:refs/tags/{selection.target_tag}')
        if args.command == 'select':
            target = (git(args.repo, 'rev-parse', f'refs/tags/{selection.target_tag}^{{commit}}').stdout.strip()
                      if selection else '')
            write_json(args.report, {'target_sha': target,
                                    'target_tag': selection.target_tag if selection else '',
                                    'skipped_tags': selection.skipped_tags if selection else []})
            output = os.environ.get('GITHUB_OUTPUT')
            if output:
                with open(output, 'a') as stream:
                    stream.write(f'target_sha={target}\n')
            return 0
        baseline = json.loads(args.baseline_report.read_text()) if args.baseline_report else None
        report = (reuse_published_candidate(args.repo, selection, manifest, base=args.base, clean_target=baseline)
                  if args.publish and selection else None)
        if report is None:
            report = prepare_update(args.repo, selection, manifest, base=args.base, clean_target=baseline)
        if report.state == 'blocked' and report.base_sha:
            if args.publish:
                reuse_published_report(args.repo, selection, report)
            prepare_blocked_report(args.repo, selection, report, base=args.base)
        write_json(args.report, asdict(report))
        if report.state == 'ready' and os.environ.get('GITHUB_OUTPUT'):
            with open(os.environ['GITHUB_OUTPUT'], 'a') as stream:
                stream.write(f'candidate_sha={report.candidate_sha}\n')
                stream.write(f'worktree={state_directory(args.repo) / "worktrees" / report.target_tag}\n')
        if args.publish and report.state != 'no_update':
            publish_draft(args.repo, report, base=args.base)
            write_json(args.report, asdict(report))
        return report.state == 'blocked'
    if args.command == 'render':
        (args.repo / 'FORK_PATCHES.md').write_text(render_registry(manifest))
        return 0
    errors = validate_patches(args.repo, manifest)
    for error in errors:
        print(error)
    if not errors:
        print('Patch registry valid')
    return bool(errors)


if __name__ == '__main__':
    raise SystemExit(main())
