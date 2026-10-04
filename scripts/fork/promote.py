#!/usr/bin/env python3
"""Read-only promotion preflight; publication requires explicit owner opt-in."""
import argparse
from dataclasses import asdict, dataclass
import hashlib
import json
from pathlib import Path
import plistlib
import re
import shutil
import struct
import subprocess
import tarfile
import tempfile

from sync import git, preparation_lock, gh_command
from verify_artifact import digest, load_baseline, reject_updater, SIDECARS

REPOSITORY = 'reynholm/buzz'
OWNER = 'reynholm'
EVIDENCE = ('owner_review', 'full_tests', 'native_about', 'existing_data',
            'sidecar_runtime', 'keychain_prompt', 'two_physical_devices', 'maintenance_command')


@dataclass
class Promotion:
    """Immutable tested source and assets; merged commit may have a different SHA."""
    candidate_sha: str
    merged_sha: str
    tree: str
    tag: str
    branch: str
    pr_url: str
    manifest_sha256: str
    checksums_sha256: str
    synthetic: bool
    asset_names: list[str]


def commit(repo: Path, sha: str) -> str:
    """Only full immutable SHAs can enter ref operations."""
    if not isinstance(sha, str) or not re.fullmatch('[0-9a-f]{40}', sha):
        raise ValueError('full lowercase commit SHA required')
    if git(repo, 'rev-parse', '--verify', sha + '^{commit}').stdout.strip() != sha:
        raise ValueError('commit SHA does not identify a commit')
    return sha


def checksums(directory: Path, names: list[str]) -> dict:
    """Validate the producer's exact four-asset checksum file, without path escapes."""
    result = {}
    for line in (directory / 'SHA256SUMS').read_text().splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  ([^/\\]+)', line)
        if not match or match[2] not in names or match[2] in result:
            raise ValueError('invalid or duplicate checksum entry')
        path = directory / match[2]
        if path.is_symlink() or not path.is_file() or path.stat().st_size <= 0 or digest(path) != match[1]:
            raise ValueError('artifact checksum mismatch: ' + match[2])
        result[match[2]] = match[1]
    if set(result) != set(names):
        raise ValueError('missing artifact checksum')
    return result


def verify_archive(archive: Path, manifest: dict):
    """Inspect archived plist/binary/resource bytes and modes without extracting or executing."""
    with tarfile.open(archive, 'r:gz') as stream:
        members = {}
        for member in stream:
            name = member.name.rstrip('/')
            if (not name.startswith('Buzz.app/') and name not in ('Buzz.app', '._Buzz.app')
                    or '..' in Path(name).parts or Path(name).is_absolute()
                    or str(Path(name)) != name or len(members) >= 10000
                    or not (member.isfile() or member.isdir()) or name in members):
                raise ValueError('unsafe or duplicate app archive entry')
            members[name] = member
        metadata_names = set()
        for name, member in members.items():
            leaf = Path(name).name
            if not leaf.startswith('._'):
                continue
            associated = str(Path(name).with_name(leaf[2:]))
            if (associated not in members or Path(associated).name.startswith('._')
                    or not member.isfile() or member.size < 26 or member.size > 1024 * 1024):
                raise ValueError('unassociated or oversized AppleDouble metadata')
            payload = stream.extractfile(member).read()
            if payload[:8] != bytes.fromhex('0005160700020000'):
                raise ValueError('invalid AppleDouble metadata header')
            count = int.from_bytes(payload[24:26], 'big')
            table_end = 26 + 12 * count
            if count < 1 or count > 64 or table_end > len(payload):
                raise ValueError('invalid AppleDouble metadata table')
            ids, intervals = set(), []
            for index in range(count):
                entry_id, offset, length = struct.unpack('>III', payload[26+12*index:38+12*index])
                if (entry_id in ids or offset < table_end or offset + length > len(payload)
                        or entry_id not in (2, 9) or entry_id == 9 and length < 32
                        or length and any(offset < end and offset + length > start for start, end in intervals)):
                    raise ValueError('invalid AppleDouble metadata entry bounds')
                ids.add(entry_id)
                if length:
                    intervals.append((offset, offset + length))
            metadata_names.add(name)
        plist_member = members.get('Buzz.app/Contents/Info.plist')
        if not plist_member or plist_member.size > 1024 * 1024:
            raise ValueError('missing or oversized app plist')
        plist = plistlib.loads(stream.extractfile(plist_member).read())
        if (plist.get('CFBundleShortVersionString') != manifest['version']
                or plist.get('CFBundleIdentifier') != manifest['identifier']
                or plist.get('CFBundleExecutable') != 'buzz-desktop'):
            raise ValueError('archived app identity mismatch')
        expected = {'Buzz.app/Contents/MacOS/' + name: record for name, record in manifest['binaries'].items()}
        expected.update({'Buzz.app/' + name: record for name, record in manifest['resources'].items()})
        actual = {name for name, member in members.items() if member.isfile() and name not in metadata_names
                  and (name.startswith('Buzz.app/Contents/MacOS/') or name.startswith('Buzz.app/Contents/Resources/'))}
        if actual != set(expected):
            raise ValueError('archive binary/resource inventory differs from manifest')
        for name, record in expected.items():
            member = members.get(name)
            if not member or member.size != record.get('size'):
                raise ValueError('archive size mismatch')
            content = stream.extractfile(member)
            if name.startswith('Buzz.app/Contents/MacOS/'):
                header = content.read(32)
                if (len(header) != 32 or header[:8] != bytes.fromhex('cffaedfe0c000001')
                        or header[12:16] != bytes.fromhex('02000000') or not member.mode & 0o111
                        or oct(member.mode & 0o777) != record.get('mode') or record.get('arch') != 'arm64'):
                    raise ValueError('archive binary architecture/mode mismatch')
                content.seek(0)
            if hashlib.file_digest(content, 'sha256').hexdigest() != record.get('sha256'):
                raise ValueError('archive checksum mismatch')
            if name.endswith(('.json', '.plist')):
                if member.size > 1024 * 1024:
                    raise ValueError('oversized structured bundle resource')
                content.seek(0)
                reject_updater(json.load(content) if name.endswith('.json') else plistlib.load(content))


def verify_promotion(repo: Path, candidate_sha: str, merged_sha: str,
                     artifact_manifest: dict) -> Promotion:
    """Require exact Git trees and independently pinned owner evidence before any mutation.

    M3 manifest fields remain unchanged. The caller adds only a transient `promotion`
    envelope with `artifact_dir` and a separately retained owner `acceptance` receipt.
    GH merge truth is checked again by publish_promotion before writing remote state.
    """
    commit(repo, candidate_sha)
    commit(repo, merged_sha)
    tree = git(repo, 'rev-parse', candidate_sha + '^{tree}').stdout.strip()
    if tree != git(repo, 'rev-parse', merged_sha + '^{tree}').stdout.strip():
        raise ValueError('merged tree differs; rebuild, retest and reaccept new candidate')
    envelope = artifact_manifest.get('promotion', {})
    acceptance = envelope.get('acceptance', {})
    if not envelope.get('artifact_dir'):
        raise ValueError('artifact directory and explicit owner acceptance are required')
    directory = Path(envelope['artifact_dir'])
    manifest = {k: v for k, v in artifact_manifest.items() if k != 'promotion'}
    if json.loads((directory / 'manifest.json').read_text()) != manifest:
        raise ValueError('manifest differs from immutable producer manifest')
    registry = json.loads(git(repo, 'show', candidate_sha + ':scripts/fork/patches.json').stdout)
    base_sha, base_tag = registry['base_sha'], registry['base_tag']
    if not re.fullmatch(r'desktop-v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)', base_tag):
        raise ValueError('invalid upstream base tag')
    if git(repo, 'rev-parse', 'refs/tags/' + base_tag + '^{commit}').stdout.strip() != base_sha:
        raise ValueError('upstream baseline tag/SHA mismatch')
    if git(repo, 'merge-base', '--is-ancestor', base_sha, candidate_sha, check=False).returncode:
        raise ValueError('candidate does not contain the upstream baseline')
    upstream = json.loads(git(repo, 'show', base_sha + ':desktop/src-tauri/tauri.conf.json').stdout)
    revision = manifest.get('fork_revision', '')
    if (manifest.get('commit_sha') != candidate_sha or manifest.get('base_tag') != base_tag
            or not isinstance(revision, str) or not re.fullmatch('[1-9][0-9]*', revision)
            or manifest.get('version') != upstream['version']
            or manifest.get('identifier') != upstream['identifier']
            or manifest.get('arch') != 'arm64' or manifest.get('updater_enabled') is not False
            or set(manifest.get('binaries', {})) != {'buzz-desktop', *SIDECARS}):
        raise ValueError('artifact identity/config does not match candidate/baseline')
    embedded = manifest.get('embedded_config', {})
    if embedded.get('version') != upstream['version'] or embedded.get('identifier') != upstream['identifier']:
        raise ValueError('embedded configuration identity mismatch')
    reject_updater(embedded)
    names = ['Buzz_' + manifest['version'] + '_aarch64' + suffix
             for suffix in ('.app.tar.gz', '.dmg')] + ['manifest.json', 'baseline.json']
    checksums(directory, names)
    baseline = load_baseline(directory / 'baseline.json', registry)
    if manifest.get('baseline') != {'upstream_sha': base_sha,
                                    'report_sha256': digest(directory / 'baseline.json'),
                                    'artifact_manifest': baseline['artifact_manifest']}:
        raise ValueError('artifact baseline provenance mismatch')
    dmg = directory / names[1]
    if manifest.get('dmg') != {'size': dmg.stat().st_size, 'sha256': digest(dmg)}:
        raise ValueError('DMG checksum/size mismatch')
    verify_archive(directory / names[0], manifest)
    expected = {'candidate_sha': candidate_sha, 'candidate_tree': tree, 'merged_sha': merged_sha,
                'base_tag': base_tag, 'upstream_sha': base_sha,
                'manifest_sha256': digest(directory / 'manifest.json'),
                'checksums_sha256': digest(directory / 'SHA256SUMS'), 'owner_login': OWNER}
    if any(acceptance.get(k) != v for k, v in expected.items()):
        raise ValueError('explicit owner merge/acceptance receipt is missing or stale')
    pr_url = acceptance.get('pr_url', '')
    if not re.fullmatch(r'https://github.com/reynholm/buzz/pull/[1-9][0-9]*', pr_url):
        raise ValueError('owner merged PR URL required')
    for name in EVIDENCE:
        record = acceptance.get('evidence', {}).get(name, {})
        if record.get('accepted') is not True or not isinstance(record.get('reference'), str) or not record['reference'].strip():
            raise ValueError('required acceptance evidence missing: ' + name)
    if git(repo, 'merge-base', '--is-ancestor', merged_sha, 'refs/heads/fork/main', check=False).returncode:
        raise ValueError('merged commit is not on accepted fork/main')
    tag = base_tag.replace('desktop-', 'fork-') + '-' + revision
    existing = git(repo, 'rev-parse', '--verify', 'refs/tags/' + tag + '^{commit}', check=False)
    if existing.returncode == 0 and existing.stdout.strip() != merged_sha:
        raise ValueError('existing final tag conflicts with merged commit')
    return Promotion(candidate_sha, merged_sha, tree, tag,
                     'fork/sync-' + base_tag.removeprefix('desktop-'), pr_url,
                     expected['manifest_sha256'], expected['checksums_sha256'],
                     acceptance.get('synthetic') is True, [*names, 'SHA256SUMS'])


def verify_owner_merge(promotion: Promotion, gh):
    """Check GitHub's authoritative merge actor, candidate head and accepted base."""
    number = promotion.pr_url.rsplit('/', 1)[1]
    pr = json.loads(gh(['gh', 'api', f'repos/{REPOSITORY}/pulls/{number}']))
    if (not pr.get('merged') or not pr.get('merged_at') or pr.get('draft')
            or pr.get('merged_by', {}).get('login') != OWNER
            or pr.get('merge_commit_sha') != promotion.merged_sha
            or pr.get('head', {}).get('sha') != promotion.candidate_sha
            or pr.get('head', {}).get('ref') != promotion.branch
            or pr.get('head', {}).get('repo', {}).get('full_name') != REPOSITORY
            or pr.get('base', {}).get('ref') != 'fork/main'
            or pr.get('base', {}).get('repo', {}).get('full_name') != REPOSITORY):
        raise ValueError('GitHub does not attest this owner-merged candidate PR')


def remote_ref(repo: Path, ref: str) -> str | None:
    """Read exact remote ref without fetching or moving accepted local refs."""
    # Annotated tags have an object SHA and a peeled commit SHA. Compare the commit.
    lines = git(repo, 'ls-remote', 'origin', ref, ref + '^{}').stdout.splitlines()
    refs = {line.split()[1]: line.split()[0] for line in lines}
    return refs.get(ref + '^{}', refs.get(ref))


def publish_promotion(repo: Path, candidate_sha: str, merged_sha: str, manifest: dict,
                      *, gh=gh_command, fixture=False) -> dict:
    """Resume only identical accepted tag/assets; delete the sync branch last.

    Fixture mode requires synthetic evidence and a local bare origin. It cannot
    address a network remote. Real publication rejects all synthetic receipts.
    """
    with preparation_lock(repo), tempfile.TemporaryDirectory(prefix='fork-promotion-') as snapshot:
        promotion = verify_promotion(repo, candidate_sha, merged_sha, manifest)
        source = Path(manifest['promotion']['artifact_dir'])
        for name in promotion.asset_names:
            shutil.copyfile(source / name, Path(snapshot) / name)
        manifest = {**manifest, 'promotion': {**manifest['promotion'], 'artifact_dir': snapshot}}
        promotion = verify_promotion(repo, candidate_sha, merged_sha, manifest)
        origin = git(repo, 'remote', 'get-url', 'origin').stdout.strip()
        if fixture:
            local = Path(origin)
            if not promotion.synthetic or not local.is_absolute() or not local.is_dir():
                raise ValueError('fixture requires synthetic acceptance and local origin')
            if git(local, 'rev-parse', '--is-bare-repository').stdout.strip() != 'true':
                raise ValueError('fixture origin must be a local bare repository')
        elif promotion.synthetic or origin not in ('https://github.com/reynholm/buzz.git',
                                                    'git@github.com:reynholm/buzz.git'):
            raise ValueError('real promotion requires owner evidence and reynholm/buzz origin')
        verify_owner_merge(promotion, gh)
        accepted = remote_ref(repo, 'refs/heads/fork/main')
        if accepted != merged_sha:
            raise ValueError('remote accepted fork/main must be the exact merged commit')
        ref = 'refs/tags/' + promotion.tag
        existing = remote_ref(repo, ref)
        if existing and existing != merged_sha:
            raise ValueError('remote final tag conflicts with merged commit')
        # Check existing release before creating a tag or uploading anything.
        pages = json.loads(gh(['gh', 'api', f'repos/{REPOSITORY}/releases?per_page=100',
                               '--paginate', '--slurp']))
        releases = [item for page in pages for item in page]
        release = next((item for item in releases if item['tag_name'] == promotion.tag), None)
        notes = ('Tested candidate: ' + candidate_sha + '\nMerged commit: ' + merged_sha
                 + '\nTree: ' + promotion.tree + '\nManifest SHA256: ' + promotion.manifest_sha256
                 + '\nSHA256SUMS SHA256: ' + promotion.checksums_sha256
                 + '\nOwner-merged PR: ' + promotion.pr_url + '\nUnsigned Apple Silicon fork.\n')
        if release and (release.get('body') != notes or release.get('target_commitish') != merged_sha):
            raise ValueError('existing release provenance conflicts')
        if not existing:
            if git(repo, 'rev-parse', '--verify', ref, check=False).returncode:
                git(repo, 'tag', promotion.tag, merged_sha)
            git(repo, 'push', 'origin', ref + ':' + ref)
        if remote_ref(repo, ref) != merged_sha:
            raise ValueError('published tag verification failed')
        directory = Path(manifest['promotion']['artifact_dir'])
        if not release:
            with tempfile.NamedTemporaryFile(mode='w', suffix='.md') as body:
                body.write(notes)
                body.flush()
                gh(['gh', 'release', 'create', promotion.tag, '--repo', REPOSITORY,
                    '--target', merged_sha, '--verify-tag', '--draft', '--title', promotion.tag,
                    '--notes-file', body.name])
        # Missing assets resume; existing bytes must match, never --clobber.
        release = json.loads(gh(['gh', 'api', f'repos/{REPOSITORY}/releases/tags/{promotion.tag}']))
        existing_names = [item['name'] for item in release['assets']]
        if len(set(existing_names)) != len(existing_names) or set(existing_names) - set(promotion.asset_names):
            raise ValueError('release contains conflicting asset inventory')
        for name in promotion.asset_names:
            if name not in existing_names:
                if not release.get('draft'):
                    raise ValueError('published release is missing verified assets')
                gh(['gh', 'release', 'upload', promotion.tag, str(directory / name), '--repo', REPOSITORY])
        with tempfile.TemporaryDirectory() as temp:
            gh(['gh', 'release', 'download', promotion.tag, '--repo', REPOSITORY, '--dir', temp])
            for name in promotion.asset_names:
                if digest(Path(temp) / name) != digest(directory / name):
                    raise ValueError('remote release asset checksum mismatch: ' + name)
        if release.get('draft'):
            gh(['gh', 'release', 'edit', promotion.tag, '--repo', REPOSITORY, '--draft=false'])
        final = json.loads(gh(['gh', 'api', f'repos/{REPOSITORY}/releases/tags/{promotion.tag}']))
        if (final.get('draft') or final.get('body') != notes or final.get('target_commitish') != merged_sha
                or set(item['name'] for item in final['assets']) != set(promotion.asset_names)
                or remote_ref(repo, ref) != merged_sha):
            raise ValueError('final release verification failed')
        branch_ref = 'refs/heads/' + promotion.branch
        branch = remote_ref(repo, branch_ref)
        if branch and branch != candidate_sha:
            raise ValueError('sync branch moved; preserve branch for owner handoff')
        if branch:
            # A deletion lease prevents racing a new candidate. It authorizes no
            # rewritten history and cannot delete a ref whose current SHA changed.
            git(repo, 'push', '--force-with-lease=' + branch_ref + ':' + candidate_sha,
                'origin', ':' + branch_ref)
        if remote_ref(repo, branch_ref):
            raise ValueError('merged sync branch deletion verification failed')
        return {**asdict(promotion), 'release_url': final['html_url'], 'state': 'published'}


def main() -> int:
    """Default is verification only; --publish is an explicit external write boundary."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path.cwd())
    parser.add_argument('--candidate', required=True)
    parser.add_argument('--merged', required=True)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--fork-revision', required=True)
    parser.add_argument('--acceptance', required=True, type=Path)
    parser.add_argument('--publish', action='store_true')
    parser.add_argument('--fixture', action='store_true', help='local bare-origin synthetic fixture only')
    args = parser.parse_args()
    try:
        manifest = json.loads(args.manifest.read_text())
        if manifest.get('fork_revision') != args.fork_revision:
            raise ValueError('fork revision differs from tested candidate')
        manifest['promotion'] = {'artifact_dir': str(args.manifest.resolve().parent),
                                 'acceptance': json.loads(args.acceptance.read_text())}
        if args.publish:
            result = publish_promotion(args.repo, args.candidate, args.merged, manifest, fixture=args.fixture)
        else:
            result = {**asdict(verify_promotion(args.repo, args.candidate, args.merged, manifest)),
                      'state': 'verified_only'}
        print(json.dumps(result, indent=2))
        return 0
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError, tarfile.TarError) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    raise SystemExit(main())
