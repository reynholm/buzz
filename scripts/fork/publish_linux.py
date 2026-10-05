#!/usr/bin/env python3
"""Attach verified Linux x86_64 assets to an existing fork release; never replace bytes."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

from sync import gh_command
from verify_artifact import digest

REPOSITORY = 'reynholm/buzz'
LINUX_ASSET_PATTERN = re.compile(r'Buzz_\d+\.\d+\.\d+_amd64\.deb|manifest-linux-amd64\.json|SHA256SUMS-linux-amd64')


def is_linux_asset(name: str) -> bool:
    """Release assets this producer owns; the macOS promotion inventory ignores them."""
    return bool(LINUX_ASSET_PATTERN.fullmatch(name))


def linux_assets(directory: Path) -> dict:
    """Exact producer inventory: the .deb and manifest pinned by the checksum file."""
    checksums = directory / 'SHA256SUMS-linux-amd64'
    result = {}
    for line in checksums.read_text().splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  ([^/\\]+)', line)
        if not match or not is_linux_asset(match[2]) or match[2] in result:
            raise ValueError('invalid or duplicate checksum entry')
        path = directory / match[2]
        if path.is_symlink() or not path.is_file() or path.stat().st_size <= 0 or digest(path) != match[1]:
            raise ValueError('artifact checksum mismatch: ' + match[2])
        result[match[2]] = match[1]
    if len(result) != 2 or 'manifest-linux-amd64.json' not in result:
        raise ValueError('checksum file must pin exactly the .deb and the manifest')
    result[checksums.name] = digest(checksums)
    return result


def verify_release(directory: Path, tag: str, gh) -> dict:
    """The manifest, the tag and GitHub's release must describe one commit."""
    manifest = json.loads((directory / 'manifest-linux-amd64.json').read_text())
    if manifest.get('release_tag') != tag or manifest.get('platform') != 'linux' \
            or manifest.get('arch') != 'x86_64' or manifest.get('updater_enabled') is not False:
        raise ValueError('manifest was not produced for this release tag')
    tag_sha = json.loads(gh(['gh', 'api', f'repos/{REPOSITORY}/git/ref/tags/{tag}']))['object']
    if tag_sha.get('type') == 'tag':
        tag_sha = json.loads(gh(['gh', 'api', f'repos/{REPOSITORY}/git/tags/{tag_sha["sha"]}']))['object']
    if tag_sha.get('type') != 'commit' or tag_sha.get('sha') != manifest.get('commit_sha'):
        raise ValueError('remote tag does not point at the manifest commit')
    release = json.loads(gh(['gh', 'api', f'repos/{REPOSITORY}/releases/tags/{tag}']))
    if release.get('draft') or release.get('tag_name') != tag:
        raise ValueError('published release for the tag is required')
    return release


def publish(directory: Path, tag: str, *, gh=gh_command) -> dict:
    """Upload only missing assets; identical bytes resume, different bytes stop."""
    assets = linux_assets(directory)
    release = verify_release(directory, tag, gh)
    existing = {item['name']: item for item in release.get('assets', [])}
    uploaded = []
    for name, checksum in assets.items():
        if name in existing:
            with tempfile.TemporaryDirectory() as temp:
                gh(['gh', 'release', 'download', tag, '--repo', REPOSITORY, '--pattern', name, '--dir', temp])
                if digest(Path(temp) / name) != checksum:
                    raise ValueError('existing release asset differs; never clobber: ' + name)
            continue
        gh(['gh', 'release', 'upload', tag, str(directory / name), '--repo', REPOSITORY])
        uploaded.append(name)
    with tempfile.TemporaryDirectory() as temp:
        for name, checksum in assets.items():
            gh(['gh', 'release', 'download', tag, '--repo', REPOSITORY, '--pattern', name, '--dir', temp])
            if digest(Path(temp) / name) != checksum:
                raise ValueError('remote release asset checksum mismatch: ' + name)
    return {'tag': tag, 'release_url': release.get('html_url'), 'uploaded': uploaded,
            'assets': assets, 'state': 'published'}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--artifacts', required=True, type=Path)
    parser.add_argument('--publish', action='store_true', help='explicitly authorize the upload')
    args = parser.parse_args()
    try:
        if not TAG_OK.fullmatch(args.tag):
            raise ValueError('tag must be fork-vX.Y.Z-N')
        if args.publish:
            result = publish(args.artifacts, args.tag)
        else:
            result = {'tag': args.tag, 'assets': linux_assets(args.artifacts), 'state': 'verified_only'}
        print(json.dumps(result, indent=2))
        return 0
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, str(error) + '\n')


TAG_OK = re.compile(r'fork-v\d+\.\d+\.\d+-[1-9][0-9]*')

if __name__ == '__main__':
    raise SystemExit(main())
