"""Host guard tests use command stubs and disposable directories, never a real mount."""
import os
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / 'deploy/scripts/storage.sh'

class StorageGuardTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='mc-guard-')
        self.root = pathlib.Path(self.tmp.name)
        self.disk = self.root / 'disk'
        self.disk.mkdir()
        for name in ['files', '.magic-cloud/database', '.magic-cloud/uploads', '.magic-cloud/trash', '.magic-cloud/thumbnails', '.magic-cloud/backups', '.magic-cloud/operations']:
            (self.disk / name).mkdir(parents=True, exist_ok=True)
        (self.disk / '.magic-cloud/storage-id').write_text('known-marker\n')
        self.config = self.root / '.env'
        self.config.write_text(f'STORAGE_MOUNT={self.disk}\nSTORAGE_UUID=abcd-1234\nSTORAGE_ID=known-marker\nAPP_UID=10001\nAPP_GID=10001\n')
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.stub('mountpoint', 'exit "${TEST_MOUNT_FAILURE:-0}"')
        self.stub('findmnt', '''case "${@: -1}" in
UUID) echo "${TEST_UUID:-abcd-1234}";;
FSTYPE) echo "${TEST_FS:-ext4}";;
OPTIONS) echo rw,nodev,nosuid;;
ID) echo 42;;
ID,MAJ:MIN) echo "42 8:1";;
esac''')
        self.env = dict(os.environ, PATH=str(self.bin)+':'+os.environ['PATH'], MC_CONFIG=str(self.config))
    def tearDown(self):
        self.tmp.cleanup()
    def stub(self, name, body):
        path = self.bin / name
        path.write_text('#!/bin/bash\n'+body+'\n')
        path.chmod(0o755)
    def check(self, **variables):
        return subprocess.run(['bash', str(SCRIPT), 'check'], env=dict(self.env, **variables), capture_output=True, text=True).returncode
    def test_correct_identity(self):
        self.assertEqual(self.check(), 0)
    def test_unmounted_directory_is_refused(self):
        self.assertNotEqual(self.check(TEST_MOUNT_FAILURE='1'), 0)
    def test_local_directory_does_not_require_mountpoint_uuid_or_ext4(self):
        with self.config.open('a') as config:
            config.write('STORAGE_MODE=local\n')
        self.assertEqual(self.check(TEST_MOUNT_FAILURE='1', TEST_UUID='other', TEST_FS='xfs'), 0)
        (self.disk / '.magic-cloud/storage-id').unlink()
        self.assertNotEqual(self.check(), 0)
    def test_local_recovery_never_mounts_or_unmounts_missing_directory(self):
        with self.config.open('a') as config:
            config.write('STORAGE_MODE=local\n')
        (self.disk / '.magic-cloud/storage-id').unlink()
        runtime = self.root / 'run'
        (runtime / 'lock').mkdir(parents=True)
        calls = self.root / 'calls'
        for command in ('docker', 'mount', 'umount'):
            self.stub(command, f'echo {command} "$*" >> "$TEST_CALLS"')
        result = subprocess.run(['bash', str(SCRIPT), 'reconcile'],
                                env=dict(self.env, MC_RUN_DIR=str(runtime), TEST_CALLS=str(calls)),
                                capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('stop -t 5 magic-cloud', calls.read_text())
        self.assertNotIn('mount ', calls.read_text())
        self.assertNotIn(' up ', calls.read_text())
    def test_invalid_storage_mode_is_refused(self):
        with self.config.open('a') as config:
            config.write('STORAGE_MODE=invalid\n')
        self.assertNotEqual(self.check(), 0)
    def test_wrong_uuid_and_filesystem(self):
        self.assertNotEqual(self.check(TEST_UUID='wrong-disk'), 0)
        self.assertNotEqual(self.check(TEST_FS='vfat'), 0)
    def test_changed_marker_is_refused(self):
        (self.disk / '.magic-cloud/storage-id').write_text('wrong-marker')
        self.assertNotEqual(self.check(), 0)
    def test_symlink_directory_is_refused(self):
        (self.disk / 'files').rmdir()
        (self.disk / 'files').symlink_to(self.root, target_is_directory=True)
        self.assertNotEqual(self.check(), 0)
    def test_initialization_is_explicit_and_does_not_replace_data(self):
        result = subprocess.run(['bash', str(SCRIPT), 'init'], env=self.env, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        result = subprocess.run(['bash', str(SCRIPT), 'init', '--initialize-empty'], env=self.env, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.disk / '.magic-cloud/storage-id').read_text(), 'known-marker\n')
    def test_reconcile_recreates_only_once_on_same_mount(self):
        runtime = self.root / 'run'
        (runtime / 'lock').mkdir(parents=True)
        calls = self.root / 'docker-calls'
        self.stub('docker', 'printf "%s\\n" "$*" >> "$TEST_DOCKER_CALLS"')
        env = dict(self.env, MC_RUN_DIR=str(runtime), TEST_DOCKER_CALLS=str(calls))
        first = subprocess.run(['bash', str(SCRIPT), 'reconcile'], env=env, capture_output=True)
        self.assertEqual(first.returncode, 0, first.stderr)
        second = subprocess.run(['bash', str(SCRIPT), 'reconcile'], env=env, capture_output=True)
        self.assertNotEqual(second.returncode, 0)
        self.assertEqual(calls.read_text().count('--force-recreate'), 1)
        self.assertIn('stop -t 5 magic-cloud', calls.read_text())

    def test_config_is_not_executed(self):
        sentinel = self.root / 'executed'
        with self.config.open('a') as f:
            f.write(f'UNUSED=$(touch {sentinel})\n')
        self.assertEqual(self.check(), 0)
        self.assertFalse(sentinel.exists())

if __name__ == '__main__':
    unittest.main()
