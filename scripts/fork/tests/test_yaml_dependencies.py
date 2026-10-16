"""Shipping lockfiles must not reintroduce the retired YAML parser graph."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[3]


class YamlDependencyTests(unittest.TestCase):
    def test_both_workspaces_exclude_retired_yaml_packages(self):
        for relative in ('Cargo.lock', 'desktop/src-tauri/Cargo.lock'):
            with self.subTest(lockfile=relative):
                packages = tomllib.loads((ROOT / relative).read_text())['package']
                names = {package['name'] for package in packages}
                self.assertFalse(names & {'unsafe-libyaml', 'serde_yaml'},
                                 f'{relative}: retired YAML dependency remains')

    def test_advisory_is_enforced_without_release_exception(self):
        ignored = tomllib.loads((ROOT / 'deny.toml').read_text())['advisories']['ignore']
        ids = {item if isinstance(item, str) else item['id'] for item in ignored}
        self.assertNotIn('RUSTSEC-2024-0449', ids)
