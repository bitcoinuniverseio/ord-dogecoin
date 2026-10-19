"""Compose retained-set pre-start guards. No I/O, launch or native admission.

This decision is valid only for the supplied fresh snapshot. A supervisor must
recollect under its held lease before storage start, and separately qualify the
mounted export and native lifetime before opening any database.
"""
from archive_recovery_admission import assess
from archive_recovery_identity import assess_identity, MACHINE_ID, GID


def assess_preparation(manifest, observed, identity, plan, now):
    """Bind identity, file ceiling and reviewed reserve to one preflight result.

    plan is private structured input, never arbitrary executable argv. Neither a
    synthetic exhaustion receipt nor a free-space calculation is a reservation.
    Runtime enforcement and external-writer exclusion require their own proof.
    """
    result = assess(manifest, observed, now)
    errors = list(result['refusals'])
    if (not assess_identity(identity, now)['identity_qualified']
            or manifest.get('machine_id') != MACHINE_ID
            or observed.get('machine_id') != MACHINE_ID):
        errors.append('dedicated_worker_identity')
    ceiling = plan.get('max_file_bytes')
    maximum = manifest.get('max_delta_allocated_bytes')
    current = observed.get('delta_identity', {}).get('bytes')
    # Sparse-file allocation cannot exceed its logical ceiling. Use the SAME
    # ceiling in reserve arithmetic and the eventual storage argv.
    if (type(ceiling) is not int or type(maximum) is not int
            or type(current) is not int
            or not 262144 <= current <= ceiling <= 600_000_000_000
            or maximum != ceiling):
        errors.append('storage_ceiling_binding')
    if type(plan.get('worker_gid')) is not int or plan['worker_gid'] != GID:
        errors.append('storage_worker_group')
    for key in ('capacity_policy_reviewed', 'reserve_enforcement_verified',
                'external_writer_exclusion_verified'):
        if observed.get(key) is not True:
            errors.append(key)
    return {'storage_preparation_qualified': not errors, 'refusals': errors,
            'production_admission': False, 'native_activation_performed': False,
            'native_health_verified': False}
