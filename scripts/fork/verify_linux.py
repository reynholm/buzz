#!/usr/bin/env python3
"""Fail closed on Linux x86_64 fork packages; probe compiled identity from the actual .deb."""
import argparse
from dataclasses import asdict, dataclass
import json
import os
from pathlib import Path
import re
import stat
import struct
import subprocess
import tempfile

from verify_artifact import digest, probe_artifact, reject_updater, SIDECARS

# Ubuntu 22.04 / Linux Mint 21 ship glibc 2.35. Upstream Linux releases are built on
# Ubuntu 24.04 and need GLIBC_2.38 (block/buzz#6157); fork assets must stay below that.
GLIBC_CEILING = (2, 35)
ARCH = 'x86_64'
TAG_PATTERN = re.compile(r'fork-v(\d+\.\d+\.\d+)-([1-9][0-9]*)')
PACKAGE_NAME = 'buzz'


@dataclass
class LinuxManifest:
    """Verified package content and immutable compiled identity."""
    commit_sha: str
    fork_revision: str
    base_tag: str
    release_tag: str | None
    platform: str
    arch: str
    glibc_ceiling: str
    version: str
    identifier: str
    control: dict
    binaries: dict
    resources: dict
    embedded_config: dict
    updater_enabled: bool
    deb: dict


def glibc_requirement(data: bytes) -> tuple[int, int] | None:
    """Highest GLIBC_x.y symbol version the binary references, or None when static."""
    versions = {(int(a), int(b)) for a, b in re.findall(rb'GLIBC_(\d+)\.(\d+)\b', data)}
    return max(versions) if versions else None


def elf_record(path: Path) -> dict:
    """Require a nonempty executable x86_64 ELF binary (no shell stubs) within the glibc ceiling."""
    if path.is_symlink() or not path.is_file():
        raise ValueError(f'missing or linked binary: {path.name}')
    info = path.stat()
    data = path.read_bytes()
    header = data[:20]
    if (len(header) != 20 or not info.st_mode & 0o111 or header[:4] != b'\x7fELF'
            or header[4] != 2 or header[5] != 1 or struct.unpack('<H', header[16:18])[0] not in (2, 3)
            or struct.unpack('<H', header[18:20])[0] != 0x3e):
        raise ValueError(f'binary is not an executable x86_64 ELF: {path.name}')
    glibc = glibc_requirement(data)
    if glibc is not None and glibc > GLIBC_CEILING:
        raise ValueError(f'{path.name} requires GLIBC_{glibc[0]}.{glibc[1]}, above the '
                         f'{GLIBC_CEILING[0]}.{GLIBC_CEILING[1]} ceiling')
    return {'size': info.st_size, 'sha256': digest(path), 'mode': oct(stat.S_IMODE(info.st_mode)),
            'arch': ARCH, 'glibc': None if glibc is None else f'{glibc[0]}.{glibc[1]}'}


def expected_identity(candidate_sha: str, fork_revision: str, base_tag: str,
                      release_tag: str | None) -> dict:
    """Identity the compiled package must carry; a release tag must agree with it."""
    if not re.fullmatch('[0-9a-f]{40}', candidate_sha):
        raise ValueError('candidate SHA must be an immutable full lowercase SHA')
    if not re.fullmatch('[1-9][0-9]*', fork_revision):
        raise ValueError('fork revision must be a positive integer')
    if not re.fullmatch(r'desktop-v\d+\.\d+\.\d+', base_tag):
        raise ValueError('invalid upstream base tag')
    if release_tag is not None:
        match = TAG_PATTERN.fullmatch(release_tag)
        if not match or match[2] != fork_revision or 'desktop-v' + match[1] != base_tag:
            raise ValueError('release tag does not match the fork revision and upstream base')
    return {'commit_sha': candidate_sha, 'fork_revision': fork_revision, 'base_tag': base_tag}


def verify_probe(probe: dict, identity: dict, upstream: dict) -> dict:
    """Compiled identity, upstream app identity and disabled updater from the native probe."""
    if probe.get('identity') != identity:
        raise ValueError('compiled fork identity does not match the exact candidate')
    config = probe.get('config', {})
    if config.get('version') != upstream['version'] or config.get('identifier') != upstream['identifier']:
        raise ValueError('embedded Tauri configuration differs from the upstream base')
    if probe.get('updater_enabled') is not False or probe.get('demo_slug') is not None:
        raise ValueError('production candidate enables updater or has a demo identity')
    reject_updater(config)
    return config


def control_fields(deb: Path) -> dict:
    """Package metadata as dpkg reads it."""
    text = subprocess.run(['dpkg-deb', '--field', str(deb)], check=True, text=True,
                          capture_output=True).stdout
    fields = {}
    for line in text.splitlines():
        if line[:1].isspace() or ':' not in line:
            continue
        key, value = line.split(':', 1)
        fields[key.strip()] = value.strip()
    return fields


def verify_deb(deb: Path, identity: dict, upstream: dict, release_tag: str | None) -> LinuxManifest:
    """Inspect the actual package: control, every binary, resources and the compiled probe."""
    if deb.is_symlink() or not deb.is_file() or deb.stat().st_size <= 0:
        raise ValueError('missing real .deb')
    expected_name = f"Buzz_{upstream['version']}_amd64.deb"
    if deb.name != expected_name:
        raise ValueError(f'package must be named {expected_name}')
    control = control_fields(deb)
    if (control.get('Package') != PACKAGE_NAME or control.get('Version') != upstream['version']
            or control.get('Architecture') != 'amd64'):
        raise ValueError('package control identity differs from the upstream base')
    with tempfile.TemporaryDirectory(prefix='fork-linux-verify-') as temp:
        root = Path(temp) / 'root'
        subprocess.run(['dpkg-deb', '-x', str(deb), str(root)], check=True)
        binaries = root / 'usr/bin'
        names = ('buzz-desktop', *SIDECARS)
        for path in binaries.iterdir():
            if path.name not in names:
                raise ValueError('unexpected package binary: ' + path.name)
        records = {name: elf_record(binaries / name) for name in names}
        probe = probe_artifact(binaries / 'buzz-desktop')
        config = verify_probe(probe, identity, upstream)
        resources = {}
        for path in sorted(root.rglob('*')):
            if path.is_symlink():
                raise ValueError('linked package entry: ' + str(path.relative_to(root)))
            if path.is_file() and not path.is_relative_to(binaries):
                if path.suffix == '.json':
                    reject_updater(json.loads(path.read_text()))
                resources[str(path.relative_to(root))] = {'size': path.stat().st_size,
                                                          'sha256': digest(path)}
    return LinuxManifest(identity['commit_sha'], identity['fork_revision'], identity['base_tag'],
                         release_tag, 'linux', ARCH, f'{GLIBC_CEILING[0]}.{GLIBC_CEILING[1]}',
                         upstream['version'], upstream['identifier'], control, records, resources,
                         config, False, {'name': deb.name, 'size': deb.stat().st_size,
                                         'sha256': digest(deb)})


def upstream_identity(repo: Path, base_sha: str) -> dict:
    """App version and bundle identifier are the upstream base's, never the fork's."""
    config = json.loads(subprocess.check_output(
        ['git', 'show', base_sha + ':desktop/src-tauri/tauri.conf.json'], cwd=repo, text=True))
    return {'version': config['version'], 'identifier': config['identifier']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate-sha', required=True)
    parser.add_argument('--fork-revision', required=True)
    parser.add_argument('--base-tag', required=True)
    parser.add_argument('--tag', help='published fork release tag the package is built for')
    parser.add_argument('--deb', required=True, type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    registry = json.loads((args.repo / 'scripts/fork/patches.json').read_text())
    if args.base_tag != registry['base_tag']:
        raise ValueError('base tag differs from the tooling registry')
    identity = expected_identity(args.candidate_sha, args.fork_revision, args.base_tag, args.tag)
    manifest = asdict(verify_deb(args.deb, identity, upstream_identity(args.repo, registry['base_sha']),
                                 args.tag))
    if args.output:
        args.output.write_text(json.dumps(manifest, indent=2) + '\n')
    print(f"Verified {manifest['deb']['name']} for {identity['commit_sha']} "
          f"(fork revision {identity['fork_revision']}, base {identity['base_tag']}, "
          f"glibc <= {manifest['glibc_ceiling']})")


if __name__ == '__main__':
    main()
