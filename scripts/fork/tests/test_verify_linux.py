"""Linux x86_64 package gates: ELF/glibc floor, identity, release tag and no-clobber publication."""
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import promote  # noqa: E402
import publish_linux  # noqa: E402
import verify_linux  # noqa: E402

SHA = 'c' * 40
UPSTREAM = {'version': '0.5.26', 'identifier': 'xyz.block.buzz.app'}


def elf_bytes(machine=0x3e, elf_class=2, kind=3, glibc=(b'GLIBC_2.34', b'GLIBC_2.17')):
    header = b'\x7fELF' + bytes([elf_class, 1, 1, 0]) + b'\0' * 8 + struct.pack('<HH', kind, machine)
    return header + b'\0' * 44 + b'\0'.join(glibc) + b'\0'


class ElfRecordTests(unittest.TestCase):
    def write(self, data, mode=0o755):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name) / 'buzz-desktop'
        path.write_bytes(data)
        path.chmod(mode)
        return path

    def test_executable_x86_64_elf_within_ceiling_is_recorded(self):
        record = verify_linux.elf_record(self.write(elf_bytes()))
        self.assertEqual((record['arch'], record['glibc'], record['mode']), ('x86_64', '2.34', '0o755'))
        self.assertGreater(record['size'], 0)

    def test_mach_o_wrong_arch_class_or_stub_is_rejected(self):
        for data in (bytes.fromhex('cffaedfe0c000001') + b'\0' * 60, elf_bytes(machine=0xb7),
                     elf_bytes(elf_class=1), elf_bytes(kind=1), b'#!/bin/sh\nexit 0\n'):
            with self.assertRaises(ValueError):
                verify_linux.elf_record(self.write(data))
        with self.assertRaises(ValueError):
            verify_linux.elf_record(self.write(elf_bytes(), mode=0o644))

    def test_glibc_above_ubuntu_22_04_floor_is_rejected(self):
        with self.assertRaises(ValueError) as caught:
            verify_linux.elf_record(self.write(elf_bytes(glibc=(b'GLIBC_2.35', b'GLIBC_2.38'))))
        self.assertIn('GLIBC_2.38', str(caught.exception))
        self.assertEqual(verify_linux.elf_record(self.write(elf_bytes(glibc=(b'GLIBC_2.35',))))['glibc'], '2.35')
        self.assertIsNone(verify_linux.elf_record(self.write(elf_bytes(glibc=())))['glibc'])


class IdentityTests(unittest.TestCase):
    def test_release_tag_must_agree_with_revision_and_base(self):
        identity = verify_linux.expected_identity(SHA, '2', 'desktop-v0.5.26', 'fork-v0.5.26-2')
        self.assertEqual(identity, {'commit_sha': SHA, 'fork_revision': '2', 'base_tag': 'desktop-v0.5.26'})
        self.assertEqual(verify_linux.expected_identity(SHA, '1', 'desktop-v0.5.26', None)['fork_revision'], '1')
        for revision, base, tag in (('1', 'desktop-v0.5.26', 'fork-v0.5.26-2'),
                                    ('2', 'desktop-v0.5.27', 'fork-v0.5.26-2'),
                                    ('2', 'desktop-v0.5.26', 'desktop-v0.5.26'),
                                    ('0', 'desktop-v0.5.26', None), ('2', 'v0.5.26', None)):
            with self.assertRaises(ValueError):
                verify_linux.expected_identity(SHA, revision, base, tag)
        with self.assertRaises(ValueError):
            verify_linux.expected_identity('c' * 39, '2', 'desktop-v0.5.26', None)

    def test_probe_identity_config_and_updater_are_enforced(self):
        identity = verify_linux.expected_identity(SHA, '2', 'desktop-v0.5.26', 'fork-v0.5.26-2')
        good = {'identity': identity, 'config': {**UPSTREAM, 'plugins': {}}, 'updater_enabled': False,
                'demo_slug': None}
        self.assertEqual(verify_linux.verify_probe(good, identity, UPSTREAM)['version'], '0.5.26')
        for bad in ({**good, 'identity': {**identity, 'fork_revision': '1'}},
                    {**good, 'identity': {**identity, 'commit_sha': 'd' * 40}},
                    {**good, 'config': {**UPSTREAM, 'version': '0.5.27'}},
                    {**good, 'config': {**UPSTREAM, 'identifier': 'xyz.block.buzz.fork'}},
                    {**good, 'updater_enabled': True}, {**good, 'demo_slug': 'demo'},
                    {**good, 'config': {**UPSTREAM, 'plugins': {'updater': {'endpoints': ['https://x']}}}}):
            with self.assertRaises(ValueError):
                verify_linux.verify_probe(bad, identity, UPSTREAM)


class PublicationTests(unittest.TestCase):
    def artifacts(self, release_tag='fork-v0.5.26-2'):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        (root / 'Buzz_0.5.26_amd64.deb').write_bytes(b'deb-bytes')
        manifest = {'release_tag': release_tag, 'platform': 'linux', 'arch': 'x86_64',
                    'updater_enabled': False, 'commit_sha': SHA}
        (root / 'manifest-linux-amd64.json').write_text(json.dumps(manifest))
        lines = [f"{verify_linux.digest(root / name)}  {name}" for name in ('Buzz_0.5.26_amd64.deb', 'manifest-linux-amd64.json')]
        (root / 'SHA256SUMS-linux-amd64').write_text('\n'.join(lines) + '\n')
        return root

    def fake_gh(self, root, existing=(), tag_sha=SHA, draft=False, remote_bytes=None):
        calls = []

        def gh(argv):
            calls.append(argv)
            if argv[:2] == ['gh', 'api']:
                if '/git/ref/tags/' in argv[2]:
                    return json.dumps({'object': {'type': 'commit', 'sha': tag_sha}})
                if '/releases/tags/' in argv[2]:
                    return json.dumps({'tag_name': 'fork-v0.5.26-2', 'draft': draft, 'html_url': 'https://example/r',
                                       'assets': [{'name': name} for name in existing]})
            if argv[:3] == ['gh', 'release', 'download']:
                name, target = argv[argv.index('--pattern') + 1], Path(argv[argv.index('--dir') + 1])
                (target / name).write_bytes(remote_bytes.get(name) if remote_bytes and name in remote_bytes
                                            else (root / name).read_bytes())
                return ''
            if argv[:3] == ['gh', 'release', 'upload']:
                return ''
            raise AssertionError('unexpected gh call: ' + ' '.join(argv))
        return gh, calls

    def test_linux_assets_are_exactly_the_pinned_producer_inventory(self):
        root = self.artifacts()
        self.assertEqual(set(publish_linux.linux_assets(root)),
                         {'Buzz_0.5.26_amd64.deb', 'manifest-linux-amd64.json', 'SHA256SUMS-linux-amd64'})
        (root / 'Buzz_0.5.26_amd64.deb').write_bytes(b'tampered')
        with self.assertRaises(ValueError):
            publish_linux.linux_assets(root)

    def test_upload_only_missing_assets_and_verify_remote_bytes(self):
        root = self.artifacts()
        gh, calls = self.fake_gh(root, existing=('Buzz_0.5.26_aarch64.dmg', 'SHA256SUMS'))
        result = publish_linux.publish(root, 'fork-v0.5.26-2', gh=gh)
        uploads = [argv for argv in calls if argv[:3] == ['gh', 'release', 'upload']]
        self.assertEqual(len(uploads), 3)
        self.assertEqual(result['state'], 'published')
        gh, calls = self.fake_gh(root, existing=('Buzz_0.5.26_amd64.deb', 'manifest-linux-amd64.json',
                                                 'SHA256SUMS-linux-amd64'))
        result = publish_linux.publish(root, 'fork-v0.5.26-2', gh=gh)
        self.assertEqual(result['uploaded'], [])
        self.assertFalse([argv for argv in calls if argv[:3] == ['gh', 'release', 'upload']])

    def test_different_existing_bytes_wrong_tag_or_draft_stop_before_upload(self):
        root = self.artifacts()
        gh, calls = self.fake_gh(root, existing=('Buzz_0.5.26_amd64.deb',),
                                 remote_bytes={'Buzz_0.5.26_amd64.deb': b'other'})
        with self.assertRaises(ValueError):
            publish_linux.publish(root, 'fork-v0.5.26-2', gh=gh)
        self.assertFalse([argv for argv in calls if argv[:3] == ['gh', 'release', 'upload']])
        for kwargs in ({'tag_sha': 'd' * 40}, {'draft': True}):
            gh, calls = self.fake_gh(root, **kwargs)
            with self.assertRaises(ValueError):
                publish_linux.publish(root, 'fork-v0.5.26-2', gh=gh)
            self.assertFalse([argv for argv in calls if argv[:3] == ['gh', 'release', 'upload']])
        gh, calls = self.fake_gh(self.artifacts(release_tag='fork-v0.5.26-1'))
        with self.assertRaises(ValueError):
            publish_linux.publish(root, 'fork-v0.5.26-1', gh=gh)

    def test_macos_promotion_inventory_ignores_linux_assets_only(self):
        for name in ('Buzz_0.5.26_amd64.deb', 'manifest-linux-amd64.json', 'SHA256SUMS-linux-amd64'):
            self.assertTrue(promote.is_linux_asset(name))
        for name in ('Buzz_0.5.26_aarch64.dmg', 'manifest.json', 'SHA256SUMS', 'Buzz_0.5.26_amd64.deb.sig',
                     'Buzz_0.5.26_arm64.deb', 'extra.txt'):
            self.assertFalse(promote.is_linux_asset(name))


if __name__ == '__main__':
    unittest.main()
