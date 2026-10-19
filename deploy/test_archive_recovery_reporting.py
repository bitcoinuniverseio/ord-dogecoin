import unittest
from archive_recovery_reporting import interpret


def queued():
    return dict(checked_unix=1000,properties=dict(ActiveState='active',MainPID='17'),
                receipt=dict(state='waiting_for_conservative_resource_admission',queue_pid=17,
                             native_database_opens=0,calm_samples=0,required_calm_samples=3))


class Reporting(unittest.TestCase):
    def test_active_queue_is_not_native_or_application_health(self):
        r=interpret(queued(),1000);self.assertEqual(r['state'],'queued_resource_wait')
        self.assertEqual(r['native_database_opens'],0);self.assertFalse(r['application_ready'])

    def test_supervisor_exit_cannot_hide_live_owned_native(self):
        s=queued();s['properties']['ActiveState']='inactive'
        s['process_observations']=dict(native=dict(pid=44,pid_present=True,identity_qualified=True,lease_descriptor_present=True))
        r=interpret(s,1000);self.assertEqual(r['state'],'owned_native_running_preserve')
        self.assertFalse(r['restart_authorized']);self.assertFalse(r['signal_authorized'])

    def test_reused_pid_never_qualifies_owner(self):
        s=queued();s['process_observations']=dict(native=dict(pid=44,pid_present=True,identity_qualified=False,lease_descriptor_present=False))
        self.assertEqual(interpret(s,1000)['state'],'native_pid_requires_identity_inspection')

    def test_phase_counter_survives_missing_final_receipt_counter(self):
        s=queued();s['receipt']=dict(state='native_attempt_requires_inspection_no_retry')
        s['phase']=dict(native_database_opens=1,native_result=dict(exit=1))
        r=interpret(s,1000);self.assertEqual(r['native_database_opens'],1)
        self.assertEqual(r['state'],'native_failed_preserve_copy_no_retry')

    def test_exit2_never_means_complete_native_or_api(self):
        s=queued();s['receipt']=dict(native_database_opens=1,native_result=dict(exit=2))
        r=interpret(s,1000);self.assertEqual(r['state'],'native_exit2_inspection_required')
        self.assertNotIn('native_integrity_percent',r);self.assertFalse(r['application_ready'])

    def test_clean_native_still_does_not_prove_application(self):
        s=queued();s['receipt']=dict(native_database_opens=1,native_result=dict(exit=0,clean_integrity_reported=True),namespace_exit=0,metadata_errors=0,corruptions=0,all_protected_mounts_removed=True,lease_released=True,storage_exit=0,qmp_acknowledged=True,loop_detached=True)
        r=interpret(s,1000);self.assertEqual(r['native_integrity_percent'],100)
        self.assertFalse(r['application_ready']);self.assertEqual(r['application_progress_percent'],0)

    def test_incomplete_storage_shutdown_cannot_complete_native_stage(self):
        s=queued();s['receipt']=dict(native_database_opens=1,native_result=dict(exit=0,clean_integrity_reported=True),namespace_exit=0,metadata_errors=0,corruptions=0,all_protected_mounts_removed=True,lease_released=True)
        self.assertNotIn('native_integrity_percent',interpret(s,1000))

    def test_stale_future_boolean_and_nonfinite_observations_refused(self):
        for t in (879,1006,True,float('nan')):
            s=queued();s['checked_unix']=t
            self.assertEqual(interpret(s,1000)['state'],'dated_observation_requires_refresh')

    def test_dead_queue_or_boolean_exit_never_reports_success(self):
        s=queued();s['properties']['ActiveState']='failed'
        self.assertEqual(interpret(s,1000)['state'],'queue_owner_requires_inspection')
        s=queued();s['receipt']=dict(state='finished',native_database_opens=1,native_result=dict(exit=False,clean_integrity_reported=True))
        self.assertEqual(interpret(s,1000)['state'],'inspection_required_preserve_owned_state')


if __name__=='__main__':unittest.main()
