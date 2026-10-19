import unittest
from test_archive_recovery_admission import inputs
from archive_recovery_identity import MACHINE_ID, NAME, UID, GID, CHECKS
from archive_recovery_preparation import assess_preparation


def fixture():
    m, s = inputs()
    m['machine_id'] = s['machine_id'] = MACHINE_ID
    i = dict(checked_unix=1000, machine_id=MACHINE_ID, name=NAME, uid=UID, gid=GID)
    i.update({k: True for k in CHECKS})
    s.update(capacity_policy_reviewed=True, reserve_enforcement_verified=True,
             external_writer_exclusion_verified=True)
    p = dict(max_file_bytes=m['max_delta_allocated_bytes'], worker_gid=GID)
    return m, s, i, p


class Preparation(unittest.TestCase):
    def test_qualified_preparation_never_admits_native(self):
        r = assess_preparation(*fixture(), 1000)
        self.assertTrue(r['storage_preparation_qualified'])
        for k in ('production_admission', 'native_activation_performed', 'native_health_verified'):
            self.assertIs(r[k], False)

    def test_real_policy_gaps_cannot_be_replaced_by_synthetic_success(self):
        for key in ('capacity_policy_reviewed', 'reserve_enforcement_verified',
                    'external_writer_exclusion_verified'):
            m, s, i, p = fixture()
            s[key] = False
            self.assertIn(key, assess_preparation(m, s, i, p, 1000)['refusals'])

    def test_ceiling_must_match_reserve_and_fit_current_delta(self):
        for cap in (True, 393215, 600_000_000_001, 599_999_999_999):
            m, s, i, p = fixture(); p['max_file_bytes'] = cap
            self.assertIn('storage_ceiling_binding', assess_preparation(m, s, i, p, 1000)['refusals'])

    def test_dedicated_identity_and_storage_group_bound_together(self):
        for change in ('stale', 'host', 'uid', 'group'):
            m, s, i, p = fixture()
            if change == 'stale': i['checked_unix'] = 879
            if change == 'host': m['machine_id'] = s['machine_id'] = 'other'
            if change == 'uid': i['uid'] = 65534
            if change == 'group': p['worker_gid'] = 65534
            self.assertFalse(assess_preparation(m, s, i, p, 1000)['storage_preparation_qualified'])

    def test_existing_snapshot_and_rpc_refusals_preserved(self):
        m, s, i, p = fixture()
        s['lease_held'] = False; s['rpc']['checked_unix'] = 879
        r = assess_preparation(m, s, i, p, 1000)
        self.assertIn('lease_held', r['refusals'])
        self.assertIn('owned_rpc', r['refusals'])


if __name__ == '__main__':
    unittest.main()
