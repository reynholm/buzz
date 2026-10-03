#!/usr/bin/env python3
"""Validate the fork patch inventory against its upstream base and live tree."""
import argparse
import json
from pathlib import Path
import re
import subprocess


def render_registry(manifest: dict) -> str:
    """Render the authoritative JSON inventory as deterministic Markdown."""
    lines = ['# Fork patches', '', 'Generated from `scripts/fork/patches.json`; regenerate with',
             '`python3 scripts/fork/sync.py render`.', '',
             f"Base: `{manifest['base_tag']}` (`{manifest['base_sha']}`).", '']
    for patch in manifest['patches']:
        lines.extend([f"## `{patch['path']}`", '', patch['responsibility'], '',
                      f"New module: `{str(patch['is_new']).lower()}`.", ''])
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
    parser.add_argument('command', choices=['validate', 'render'], nargs='?', default='validate')
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    manifest = json.loads((args.repo / 'scripts/fork/patches.json').read_text())
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
