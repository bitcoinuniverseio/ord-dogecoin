import json
import unittest
from archive_recovery_storage_plan import storage_argv


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


if __name__ == '__main__':
    unittest.main()
