"""Validate host installation plans without changing mounts or system files."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    'install_storage', Path(__file__).resolve().parents[1] / 'deploy/scripts/install-storage.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallStorageTests(unittest.TestCase):
    def test_local_install_keeps_path_and_does_not_require_uuid(self):
        original = 'STORAGE_MODE=local\nSTORAGE_MOUNT=/srv/data\nSTORAGE_ID=marker\n'
        self.assertEqual(installer.fixed_config(original), original)
        self.assertEqual(installer.configuration(original)['STORAGE_MODE'], 'local')
    def test_fstab_preserves_other_disks_and_is_idempotent(self):
        original = '# root disk\nUUID=aaaa / ext4 defaults 0 1\n'
        result = installer.fstab_config(original, 'abcd-1234')
        self.assertTrue(result.startswith(original))
        self.assertIn('UUID=abcd-1234 /mnt/magic-cloud ext4 ', result)
        self.assertEqual(installer.fstab_config(result, 'abcd-1234'), result)

    def test_fstab_refuses_conflicting_targets_and_disk_entries(self):
        for entry in ('UUID=ffff /mnt/magic-cloud ext4 defaults 0 2\n',
                      'UUID=abcd-1234 /somewhere-else ext4 defaults 0 2\n',
                      'UUID=abcd-1234 /mnt/magic-cloud vfat defaults 0 2\n'):
            with self.subTest(entry=entry), self.assertRaises(ValueError):
                installer.fstab_config(entry, 'abcd-1234')

    def test_only_mount_path_changes_in_application_config(self):
        original = ('APP_PASSWORD=keep-this-value\nSTORAGE_MOUNT=/media/user/DISK\n'
                    'STORAGE_UUID=abcd-1234\nSTORAGE_ID=existing-marker\n')
        result = installer.fixed_config(original)
        self.assertEqual(result, original.replace('/media/user/DISK', '/mnt/magic-cloud'))

    def test_duplicate_configuration_is_refused(self):
        with self.assertRaises(ValueError):
            installer.configuration('STORAGE_MOUNT=/a\nSTORAGE_MOUNT=/b\n'
                                    'STORAGE_UUID=abcd-1234\nSTORAGE_ID=marker\n')


if __name__ == '__main__':
    unittest.main()
