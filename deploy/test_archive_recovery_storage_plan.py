import json
import unittest
from unittest.mock import patch
from archive_recovery_storage_plan import storage_argv, prepare_worker_export


class StoragePlanTests(unittest.TestCase):
    def test_graph_and_limit_bound_before_daemon(self):
        args = storage_argv('/private/source', '/private/delta', '/private/view', '/private/qmp.sock', 1048576)
        self.assertEqual(args[:3], ['/usr/bin/prlimit', '--fsize=1048576:1048576', '--'])
        self.assertEqual(args[3:5], ['/usr/bin/env', '--ignore-signal=XFSZ'])
        self.assertIn('--bounding-set=-sys_resource', args)
        self.assertIn('--no-new-privs', args)
        graph = [json.loads(args[i+1]) for i, x in enumerate(args) if x == '--blockdev']
        self.assertEqual([g['read-only'] for g in graph], [True, True, False, False])
        self.assertEqual(graph[-1]['backing'], 'source')
        self.assertEqual(graph[0]['filename'], '/private/source')
        self.assertEqual(graph[2]['filename'], '/private/delta')
        self.assertIn('allow-other=off', args[-1])

    def test_alias_and_option_injection_refused(self):
        for bad in ['/private/a,server=on', 'relative', '/private/../a', '//host/a', '/a\\b', '/a\n']:
            with self.subTest(path=bad), self.assertRaises(ValueError):
                storage_argv(bad, '/d', '/v', '/q', 1048576)
        with self.assertRaises(ValueError):
            storage_argv('/a', '/a', '/v', '/q', 1048576)

    def test_invalid_or_excessive_limit_refused(self):
        for value in [True, 0, -1, 1.5, 262143, 600_000_000_001]:
            with self.subTest(limit=value), self.assertRaises(ValueError):
                storage_argv('/a', '/d', '/v', '/q', value)

    def test_worker_export_requires_guard_and_explicit_group(self):
        for gid in [0, True, -1, '65534']:
            with self.subTest(gid=gid), self.assertRaises(ValueError):
                storage_argv('/a', '/d', '/v', '/q', 1048576, worker_gid=gid)
        with self.assertRaisesRegex(ValueError, 'private_control_paths'):
            storage_argv('/a', '/d', '/private/view', '/q', 1048576, worker_gid=65534)
        with patch('archive_recovery_storage_plan._worker_export_guard', side_effect=ValueError('private_export_permissions')):
            with self.assertRaises(ValueError):
                storage_argv('/private/control/a', '/private/control/d', '/private/view', '/private/control/q', 1048576, worker_gid=65534)

    def test_active_native_prevents_permission_changes(self):
        with patch('archive_recovery_storage_plan.os.fchown', create=True) as change:
            with self.assertRaisesRegex(ValueError, 'native_not_idle'):
                prepare_worker_export('/a', '/d', '/v', '/q', 65534, lambda: False)
            change.assert_not_called()

    def test_missing_fuse_mount_never_changes_backing_permissions(self):
        with patch('archive_recovery_storage_plan._worker_export_guard'), patch('archive_recovery_storage_plan._fuse_mount', side_effect=ValueError('exact_fuse_mount_required')), patch('archive_recovery_storage_plan.os.fchown', create=True) as change:
            with self.assertRaisesRegex(ValueError, 'exact_fuse_mount_required'):
                prepare_worker_export('/a', '/d', '/v', '/q', 65534, lambda: True)
            change.assert_not_called()


if __name__ == '__main__':
    unittest.main()
