"""Retained archive successor preflight. This module never launches a service.

The caller must collect observations privately and hold OwnerLease throughout
any later reviewed successor lifetime. A passed snapshot is not native health.
"""
import math
import os
import stat

MAX_AGE_SECONDS = 120


def assess(manifest, observed, now):
    """Return stable refusal codes without echoing input values or credentials."""
    errors = []
    def require(ok, code):
        if not ok:
            errors.append(code)
    def integer(value):
        return type(value) is int and value >= 0
    timestamp = observed.get('checked_unix')
    fresh = (type(timestamp) in (int, float) and math.isfinite(timestamp)
             and type(now) in (int, float) and math.isfinite(now)
             and -5 <= now - timestamp <= MAX_AGE_SECONDS)
    require(fresh, 'stale_snapshot')
    require(observed.get('machine_id') == manifest.get('machine_id')
            and bool(manifest.get('machine_id')), 'host_identity')
    for key in ('base_identity', 'delta_identity'):
        expected, actual = manifest.get(key), observed.get(key)
        fields = ('dev', 'inode', 'bytes', 'mtime_ns', 'ctime_ns', 'uid', 'gid', 'mode', 'links')
        require(isinstance(expected, dict) and bool(expected)
                and all(integer(expected.get(k)) for k in fields)
                and expected.get('links') == 1 and expected.get('uid') == 0
                and expected.get('mode', 0) & 0o022 == 0
                and isinstance(actual, dict) and actual == expected, key)
    for key in ('runtime_sha256', 'delta_sha256'):
        expected = manifest.get(key)
        require(isinstance(expected, str) and len(expected) == 64
                and all(c in '0123456789abcdef' for c in expected)
                and observed.get(key) == expected, key)
    require(observed.get('old_native_pid') == 0
            and type(observed.get('old_native_pid')) is int
            and observed.get('old_storage_pid') == 0
            and type(observed.get('old_storage_pid')) is int
            and observed.get('view_mounted') is False, 'existing_owner')
    for key in ('lease_held', 'source_readonly_verified',
                'retained_metadata_check_verified', 'growth_exhaustion_verified'):
        require(observed.get(key) is True, key)
    rpc = observed.get('rpc', {})
    rpc_time = rpc.get('checked_unix') if isinstance(rpc, dict) else None
    require(isinstance(rpc, dict) and type(rpc_time) in (int, float)
            and math.isfinite(rpc_time) and type(now) in (int, float)
            and math.isfinite(now) and -5 <= now - rpc_time <= MAX_AGE_SECONDS
            and rpc.get('authenticated_read_passed') is True
            and rpc.get('mainnet_genesis_verified') is True
            and rpc.get('ibd') is False
            and rpc.get('transport_peer') == manifest.get('owned_rpc_peer')
            and manifest.get('owned_rpc_peer') == '51.222.108.117', 'owned_rpc')
    capacity_keys = ('free_bytes', 'delta_allocated_bytes')
    policy_keys = ('reserve_bytes', 'max_delta_allocated_bytes')
    valid = all(integer(observed.get(k)) for k in capacity_keys)
    valid = valid and all(integer(manifest.get(k)) for k in policy_keys)
    valid = valid and manifest.get('reserve_bytes', 0) >= 300_000_000_000
    if valid:
        used, maximum = observed['delta_allocated_bytes'], manifest['max_delta_allocated_bytes']
        valid = used <= maximum and maximum > 0
        valid = valid and observed['free_bytes'] >= manifest['reserve_bytes'] + maximum - used
    require(valid, 'capacity_budget')
    return {'preflight_passed': not errors, 'refusals': errors,
            'native_activation_performed': False, 'native_health_verified': False}


class OwnerLease:
    """Nonblocking, no-truncation Linux lease in an owned private directory."""
    def __init__(self, path):
        self.path, self.fd = os.fspath(path), None

    def __enter__(self):
        import fcntl
        parent = os.path.dirname(os.path.abspath(self.path))
        p = os.lstat(parent)
        if not stat.S_ISDIR(p.st_mode) or p.st_uid != os.geteuid() or p.st_mode & 0o022:
            raise PermissionError('lease_parent_guard')
        fd = os.open(self.path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
        try:
            s = os.fstat(fd)
            if (not stat.S_ISREG(s.st_mode) or s.st_nlink != 1
                    or s.st_uid != os.geteuid() or s.st_mode & 0o077):
                raise PermissionError('lease_file_guard')
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            current = os.lstat(self.path)
            if (s.st_dev, s.st_ino) != (current.st_dev, current.st_ino):
                raise PermissionError('lease_path_replaced')
        except BaseException:
            os.close(fd)
            raise
        self.fd = fd
        return self

    def __exit__(self, *unused):
        os.close(self.fd)
        self.fd = None
