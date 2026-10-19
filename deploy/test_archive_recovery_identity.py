import unittest
from unittest.mock import patch
import archive_recovery_identity as identity


def observation():
    return dict(checked_unix=1000, machine_id=identity.MACHINE_ID,
                name=identity.NAME, uid=identity.UID, gid=identity.GID,
                **{k: True for k in identity.CHECKS})


class IdentityTests(unittest.TestCase):
    def test_pinned_identity_is_scoped_only(self):
        result = identity.assess_identity(observation(), 1001)
        self.assertTrue(result['identity_qualified'])
        self.assertFalse(result['production_admission'])

    def test_changed_host_ids_or_isolation_refused(self):
        for key, value in [('machine_id', 'other'), ('name', 'nobody'),
                           ('uid', 65534), ('gid', 65534), ('uid', True),
                           *[(k, False) for k in identity.CHECKS]]:
            with self.subTest(key=key):
                row = observation(); row[key] = value
                self.assertFalse(identity.assess_identity(row, 1001)['identity_qualified'])

    def test_old_future_or_nonfinite_audit_refused(self):
        for stamp in [0, 1007, float('nan'), True]:
            row = observation(); row['checked_unix'] = stamp
            self.assertFalse(identity.assess_identity(row, 1001)['identity_qualified'])

    def test_worker_plan_reaudits_identity(self):
        with patch.object(identity, 'inspect_identity', return_value=observation()), patch.object(identity.time, 'time', return_value=1001):
            plan = identity.worker_argv('/private/native', ['--testnet', 'index'])
        self.assertIn('--reuid=981', plan)
        self.assertIn('--regid=977', plan)
        self.assertIn('--no-new-privs', plan)
        self.assertEqual(plan[-3:], ['/private/native', '--testnet', 'index'])

    def test_worker_plan_refuses_changed_identity(self):
        row = observation(); row['password_disabled'] = False
        with patch.object(identity, 'inspect_identity', return_value=row), patch.object(identity.time, 'time', return_value=1001):
            with self.assertRaisesRegex(ValueError, 'worker_identity_changed'):
                identity.worker_argv('/private/native', [])


if __name__ == '__main__':
    unittest.main()
