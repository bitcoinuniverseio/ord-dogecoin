"""Read-only interpretation of observed queue state. Never controls a worker."""
import math


def interpret(snapshot, now):
    checked = snapshot.get('checked_unix')
    fresh = (type(checked) in (int, float) and type(now) in (int, float)
             and math.isfinite(checked) and math.isfinite(now) and -5 <= now-checked <= 120)
    base = dict(application_ready=False, application_progress_percent=0,
                restart_authorized=False, signal_authorized=False)
    if not fresh:
        return dict(base, state='dated_observation_requires_refresh', native_database_opens=None)
    receipt = snapshot.get('receipt') or {}
    phase = snapshot.get('phase') or {}
    process = (snapshot.get('process_observations') or {}).get('native') or {}
    counts = [v for v in [receipt.get('native_database_opens'), phase.get('native_database_opens')]
              if type(v) is int and v in (0, 1)]
    opens = max(counts) if counts else None
    owned = process.get('identity_qualified') is True and process.get('lease_descriptor_present') is True
    if owned:
        return dict(base, state='owned_native_running_preserve', native_database_opens=1,
                    native_pid=process['pid'], native_stage_percent_estimated=10,
                    progress_kind='estimated native stage; no measured scan denominator')
    if process.get('pid_present') is True:
        return dict(base, state='native_pid_requires_identity_inspection', native_database_opens=opens)
    state = receipt.get('state', '')
    if state == 'waiting_for_conservative_resource_admission' and opens == 0:
        unit = snapshot.get('properties') or {}
        if unit.get('ActiveState') == 'active' and unit.get('MainPID') == str(receipt.get('queue_pid')):
            return dict(base, state='queued_resource_wait', native_database_opens=0,
                        native_stage_percent_estimated=0,
                        calm_samples=receipt.get('calm_samples'),
                        required_calm_samples=receipt.get('required_calm_samples'))
        return dict(base, state='queue_owner_requires_inspection', native_database_opens=0)
    result = receipt.get('native_result') or phase.get('native_result') or {}
    exit_code = result.get('exit')
    if type(exit_code) is int and exit_code == 1:
        return dict(base, state='native_failed_preserve_copy_no_retry', native_database_opens=opens,
                    native_exit=1)
    if type(exit_code) is int and exit_code == 2:
        return dict(base, state='native_exit2_inspection_required', native_database_opens=opens,
                    native_exit=2, native_stage_percent_estimated=80,
                    progress_kind='estimated qualification stage; inspection remains')
    clean = (type(exit_code) is int and exit_code == 0
             and opens == 1
             and result.get('clean_integrity_reported') is True
             and type(receipt.get('namespace_exit')) is int and receipt['namespace_exit'] == 0
             and type(receipt.get('metadata_errors')) is int and receipt['metadata_errors'] == 0
             and type(receipt.get('corruptions')) is int and receipt['corruptions'] == 0
             and receipt.get('all_protected_mounts_removed') is True
             and receipt.get('lease_released') is True
             and type(receipt.get('storage_exit')) is int and receipt['storage_exit'] == 0
             and receipt.get('qmp_acknowledged') is True and receipt.get('loop_detached') is True)
    if clean:
        return dict(base, state='native_integrity_complete_application_pending', native_database_opens=opens,
                    native_integrity_percent=100)
    if opens == 0 and state == 'one_native_attempt_preparing':
        return dict(base, state='native_launch_preparing', native_database_opens=0,
                    native_stage_percent_estimated=0)
    return dict(base, state='inspection_required_preserve_owned_state', native_database_opens=opens)
