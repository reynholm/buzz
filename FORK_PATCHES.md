# Fork patches

Generated from `scripts/fork/patches.json`; regenerate with
`python3 scripts/fork/sync.py render`.

Base: `desktop-v0.5.26` (`2b4b138dc5cf2d9cc1a0ceb21d9063ff56fe8bf4`).

## `docs/superpowers/plans/2026-10-03-buzz-fork-maintenance.md`

Approved fork maintenance implementation plan

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `docs/superpowers/plans/2026-10-03-device-bound-agents.md`

Approved device ownership implementation plan

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `docs/superpowers/specs/2026-10-03-device-bound-agents-design.md`

Approved device ownership and fork acceptance contract

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `FORK_PATCHES.md`

Reviewable generated patch inventory

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `scripts/fork/sync.py`

Validate lost patch paths, symbols and production invocation seams

New module: `true`.

- Required symbol: `def validate_patches(`
- Required symbol: `def main(`
- Invocation: `scripts/fork/sync.py` → `validate_patches`; exact call `errors = validate_patches(args.repo, manifest)`; behavior test `test_cli_rejects_unlisted_path`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `python3 scripts/fork/sync.py validate`

## `scripts/fork/patches.json`

Authoritative patch inventory

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `scripts/fork/tests/test_patch_registry.py`

Mutation coverage for the production registry validator

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `docs/fork/MAINTENANCE.md`

Owner update policy and reproducible baseline evidence

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`
