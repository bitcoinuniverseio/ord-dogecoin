import copy
import os
import pathlib
import tempfile
import unittest
from archive_recovery_admission import assess, OwnerLease


def inputs():
    base = {'dev': 1, 'inode': 11, 'bytes': 2258359144448, 'mtime_ns': 10,
            'ctime_ns': 10, 'uid': 0, 'gid': 0, 'mode': 0o644, 'links': 1}
    delta = dict(base, inode=12, bytes=393216, mode=0o600)
    manifest = {'machine_id': 'fixture-host', 'base_identity': base,
                'delta_identity': delta, 'runtime_sha256': 'a'*64, 'delta_sha256': 'b'*64,
                'owned_rpc_peer': '51.222.108.117', 'reserve_bytes': 300_000_000_000,
                'max_delta_allocated_bytes': 600_000_000_000}
    snapshot = {'checked_unix': 1000, 'machine_id': 'fixture-host',
                'base_identity': copy.deepcopy(manifest['base_identity']),
                'delta_identity': copy.deepcopy(manifest['delta_identity']),
                'runtime_sha256': 'a'*64, 'delta_sha256': 'b'*64,
                'old_native_pid': 0, 'old_storage_pid': 0, 'view_mounted': False,
                'lease_held': True, 'source_readonly_verified': True,
                'retained_metadata_check_verified': True, 'growth_exhaustion_verified': True,
                'free_bytes': 900_000_000_000, 'delta_allocated_bytes': 0,
                'rpc': {'checked_unix': 1000, 'authenticated_read_passed': True,
                        'mainnet_genesis_verified': True, 'ibd': False,
                        'transport_peer': '51.222.108.117'}}
    return manifest, snapshot


class Admission(unittest.TestCase):
    def test_exact_budget_boundary_is_preflight_only(self):
        m, s = inputs();result = assess(m, s, 1000)
        self.assertTrue(result['preflight_passed'])
        self.assertFalse(result['native_activation_performed'])
        self.assertFalse(result['native_health_verified'])

    def test_one_byte_short_and_over_budget_refused(self):
        for change in ({'free_bytes': 899_999_999_999}, {'delta_allocated_bytes': 600_000_000_001},
                       {'free_bytes': True}, {'free_bytes': -1}):
            m, s = inputs();s.update(change)
            self.assertIn('capacity_budget', assess(m, s, 1000)['refusals'])

    def test_identity_alias_and_runtime_mismatch_refused(self):
        for key, value in [('machine_id', 'other-host'), ('base_identity', {'inode': 99}),
                           ('delta_identity', {'inode': 99}), ('runtime_sha256', 'c'*64),
                           ('delta_sha256', 'd'*64)]:
            m, s = inputs();s[key] = value
            self.assertFalse(assess(m, s, 1000)['preflight_passed'])

    def test_incomplete_or_unsafe_expected_identity_refused(self):
        for field, value in [('links', 2), ('mode', 0o666), ('uid', 65534), ('dev', True)]:
            m, s = inputs();m['base_identity'][field] = value
            s['base_identity'][field] = value
            self.assertIn('base_identity', assess(m, s, 1000)['refusals'])
        m, s = inputs();m['base_identity'].pop('ctime_ns');s['base_identity'].pop('ctime_ns')
        self.assertIn('base_identity', assess(m, s, 1000)['refusals'])

    def test_running_owner_mount_and_boolean_pid_refused(self):
        for key, value in [('old_native_pid', 25), ('old_storage_pid', 26),
                           ('old_native_pid', False), ('view_mounted', True)]:
            m, s = inputs();s[key] = value
            self.assertIn('existing_owner', assess(m, s, 1000)['refusals'])

    def test_missing_lease_or_exhaustion_proof_refused(self):
        for key in ['lease_held', 'source_readonly_verified',
                    'retained_metadata_check_verified', 'growth_exhaustion_verified']:
            m, s = inputs();s.pop(key)
            self.assertIn(key, assess(m, s, 1000)['refusals'])

    def test_stale_future_and_nonfinite_receipts_refused(self):
        for timestamp in [879, 1006, float('nan'), float('inf'), True]:
            m, s = inputs();s['checked_unix'] = timestamp
            self.assertIn('stale_snapshot', assess(m, s, 1000)['refusals'])

    def test_rpc_stale_unauthenticated_or_unowned_refused(self):
        for key, value in [('checked_unix', 879), ('transport_peer', 'unowned'),
                           ('authenticated_read_passed', False), ('ibd', True),
                           ('mainnet_genesis_verified', False)]:
            m, s = inputs();s['rpc'][key] = value
            self.assertIn('owned_rpc', assess(m, s, 1000)['refusals'])

    @unittest.skipUnless(os.name == 'posix', 'Linux lease behavior')
    def test_lease_contention_preserves_existing_bytes(self):
        with tempfile.TemporaryDirectory() as root:
            p = pathlib.Path(root)/'owner.lock';p.write_bytes(b'preserve');p.chmod(0o600)
            with OwnerLease(p):
                with self.assertRaises(BlockingIOError):
                    with OwnerLease(p):pass
                self.assertEqual(p.read_bytes(), b'preserve')
            with OwnerLease(p):self.assertEqual(p.read_bytes(), b'preserve')

    @unittest.skipUnless(os.name == 'posix', 'Linux lease behavior')
    def test_symlink_hardlink_and_writable_parent_refused(self):
        with tempfile.TemporaryDirectory() as root:
            r = pathlib.Path(root);p = r/'owner.lock';p.write_bytes(b'preserve');p.chmod(0o600)
            alias = r/'alias';os.symlink(p, alias)
            with self.assertRaises(OSError):
                with OwnerLease(alias):pass
            os.unlink(alias);os.link(p, alias)
            with self.assertRaises(PermissionError):
                with OwnerLease(p):pass
            os.unlink(alias);r.chmod(0o777)
            with self.assertRaises(PermissionError):
                with OwnerLease(p):pass
            r.chmod(0o700);self.assertEqual(p.read_bytes(), b'preserve')


if __name__ == '__main__':unittest.main()
