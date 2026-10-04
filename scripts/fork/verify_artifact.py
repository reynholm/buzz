#!/usr/bin/env python3
"""Fail closed on candidate bundles; probe actual compiled config before app startup."""
import argparse
from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import stat
import struct
import subprocess

SIDECARS = ('buzz-acp', 'buzz-agent', 'buzz-backend-kubernetes', 'buzz-dev-mcp',
            'git-credential-nostr', 'buzz')


@dataclass
class ArtifactManifest:
    """Verified bundle content and immutable compiled identity."""
    commit_sha: str
    fork_revision: str
    base_tag: str
    arch: str
    version: str
    identifier: str
    binaries: dict
    resources: dict
    embedded_config: dict
    updater_enabled: bool


def digest(path: Path) -> str:
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def binary_record(path: Path) -> dict:
    """Require a nonempty executable, thin arm64 Mach-O executable (no shell stubs)."""
    if path.is_symlink() or not path.is_file():
        raise ValueError(f'missing or linked binary: {path.name}')
    info = path.stat()
    with path.open('rb') as stream:
        header = stream.read(32)
    if (len(header) != 32 or not info.st_mode & 0o111
            or struct.unpack('<I', header[:4])[0] != 0xfeedfacf
            or struct.unpack('<I', header[4:8])[0] != 0x100000c
            or struct.unpack('<I', header[12:16])[0] != 2):
        raise ValueError(f'binary is not executable arm64 Mach-O: {path.name}')
    return {'size': info.st_size, 'sha256': digest(path),
            'mode': oct(stat.S_IMODE(info.st_mode)), 'arch': 'arm64'}


def probe_artifact(executable: Path) -> dict:
    """The native probe returns before runtime, data, keychain or GUI startup."""
    result = subprocess.run([str(executable), '--fork-artifact-probe'], check=True,
                            text=True, capture_output=True, timeout=30)
    if len(result.stdout) > 1024 * 1024:
        raise ValueError('unexpectedly large native probe output')
    return json.loads(result.stdout)


def reject_updater(value):
    """Inspect actual generated config and every bundled JSON/plist resource."""
    if isinstance(value, dict):
        updater = value.get('updater')
        if isinstance(updater, dict) and (updater.get('endpoints') or updater.get('pubkey')
                                         or updater.get('active')):
            raise ValueError('bundled updater configuration is enabled')
        if value.get('createUpdaterArtifacts'):
            raise ValueError('updater artifact configuration is enabled')
        for nested in value.values():
            reject_updater(nested)
    elif isinstance(value, list):
        for nested in value:
            reject_updater(nested)


def verify_artifact(app: Path, candidate_sha: str, baseline_config: dict) -> ArtifactManifest:
    """Validate actual bundle identity/config/resources against the exact prior baseline."""
    if not re.fullmatch('[0-9a-f]{40}', candidate_sha):
        raise ValueError('candidate SHA must be an immutable full lowercase SHA')
    with (app / 'Contents/Info.plist').open('rb') as stream:
        plist = plistlib.load(stream)
    version, identifier = plist.get('CFBundleShortVersionString'), plist.get('CFBundleIdentifier')
    if version != baseline_config['version'] or identifier != baseline_config['identifier']:
        raise ValueError('upstream app version or bundle identifier changed')
    executable_name = plist.get('CFBundleExecutable')
    if not isinstance(executable_name, str) or Path(executable_name).name != executable_name:
        raise ValueError('invalid executable path')
    binaries = app / 'Contents/MacOS'
    records = {name: binary_record(binaries / name) for name in (executable_name, *SIDECARS)}
    probe = probe_artifact(binaries / executable_name)
    identity = probe.get('identity')
    expected = {'commit_sha': candidate_sha, 'base_tag': baseline_config['base_tag'],
                'fork_revision': baseline_config['fork_revision']}
    if identity != expected:
        raise ValueError('compiled fork identity does not match the exact candidate')
    config = probe.get('config', {})
    if config.get('version') != version or config.get('identifier') != identifier:
        raise ValueError('embedded Tauri configuration differs from app plist/baseline')
    if probe.get('updater_enabled') is not False or probe.get('demo_slug') is not None:
        raise ValueError('production candidate enables updater or has a demo identity')
    reject_updater(config)
    resources = {}
    for path in sorted((app / 'Contents/Resources').rglob('*')):
        if path.is_symlink():
            raise ValueError('linked bundle resource')
        if path.is_file():
            if path.suffix == '.json':
                reject_updater(json.loads(path.read_text()))
            elif path.suffix == '.plist':
                reject_updater(plistlib.loads(path.read_bytes()))
            resources[str(path.relative_to(app))] = {'size': path.stat().st_size,
                                                    'sha256': digest(path)}
    return ArtifactManifest(candidate_sha, identity['fork_revision'], identity['base_tag'],
                            'arm64', version, identifier, records, resources, config, False)


def load_baseline(report_path: Path, registry: dict) -> dict:
    """Require M2's successful exact clean baseline with all real artifact receipts."""
    report = json.loads(report_path.read_text())
    artifacts = report.get('artifact_manifest', {})
    if (report.get('upstream_sha') != registry['base_sha']
            or report.get('result') != 'success' or not report.get('gate_commands')
            or not isinstance(artifacts, dict)):
        raise ValueError('exact successful upstream baseline is required')
    required = ['macos/Buzz.app/Contents/MacOS/' + name
                for name in ('buzz-desktop', *SIDECARS)]
    if not all(name in artifacts for name in required) or not any(
            name.startswith('dmg/') and name.endswith('.dmg') for name in artifacts):
        raise ValueError('baseline is missing real app/sidecar/DMG receipts')
    for name, record in artifacts.items():
        if (Path(name).is_absolute() or '..' in Path(name).parts
                or not isinstance(record.get('size'), int) or record['size'] <= 0
                or not re.fullmatch('[0-9a-f]{64}', record.get('sha256', ''))):
            raise ValueError('invalid baseline artifact receipt')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate-sha', required=True)
    parser.add_argument('--baseline-report', required=True, type=Path)
    parser.add_argument('--app', type=Path)
    parser.add_argument('--sidecar-dir', type=Path)
    parser.add_argument('--dmg', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    registry = json.loads((repo / 'scripts/fork/patches.json').read_text())
    baseline = load_baseline(args.baseline_report, registry)
    if not re.fullmatch('[0-9a-f]{40}', args.candidate_sha):
        raise ValueError('candidate SHA must be a full lowercase SHA')
    if args.sidecar_dir:
        for name in SIDECARS:
            binary_record(args.sidecar_dir / (name + '-aarch64-apple-darwin'))
    if args.app:
        upstream_config = json.loads(subprocess.check_output(
            ['git', 'show', registry['base_sha'] + ':desktop/src-tauri/tauri.conf.json'],
            cwd=repo, text=True))
        config = {'version': upstream_config['version'], 'identifier': upstream_config['identifier'],
                  'base_tag': registry['base_tag'], 'fork_revision': os.environ['BUZZ_FORK_REVISION']}
        manifest = asdict(verify_artifact(args.app, args.candidate_sha, config))
        manifest['baseline'] = {'upstream_sha': baseline['upstream_sha'],
                                'report_sha256': digest(args.baseline_report),
                                'artifact_manifest': baseline['artifact_manifest']}
        if args.dmg:
            if args.dmg.is_symlink() or not args.dmg.is_file() or args.dmg.stat().st_size <= 0:
                raise ValueError('missing real DMG')
            subprocess.run(['hdiutil', 'verify', str(args.dmg)], check=True, capture_output=True)
            manifest['dmg'] = {'size': args.dmg.stat().st_size, 'sha256': digest(args.dmg)}
        if args.output:
            args.output.write_text(json.dumps(manifest, indent=2) + '\n')
    elif not args.sidecar_dir:
        # Preflight validates baseline and SHA without starting a build.
        print('Exact successful baseline verified')


if __name__ == '__main__':
    main()
