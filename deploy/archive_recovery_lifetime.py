"""Cooperating-owner child lifetime primitive, without production admission.

No automatic teardown: exceptions and wait deadlines never signal children.
Caller policy must qualify executable, graph, namespace, identity, capacity,
read-only source and fresh owned RPC before using spawn. Children must preserve
the passed lease descriptor; each actual executable needs that qualification.
"""
import os
import pathlib
import re
import subprocess

from archive_recovery_admission import OwnerLease
from archive_recovery_qmp import acknowledged_quit


class LifetimeRefused(RuntimeError):
    pass


class LeaseGroup:
    def __init__(self, lease_path, mountpoint):
        if not os.path.isabs(os.fspath(mountpoint)):
            raise LifetimeRefused('absolute_mountpoint_required')
        self.mountpoint = os.path.abspath(mountpoint)
        self.lease = OwnerLease(lease_path)
        self.children = {}
        self.lease.__enter__()

    def _held(self):
        if self.lease.fd is None:
            raise LifetimeRefused('lease_closed')
        held, current = os.fstat(self.lease.fd), os.lstat(self.lease.path)
        if (held.st_dev, held.st_ino) != (current.st_dev, current.st_ino):
            raise LifetimeRefused('lease_path_changed')

    def spawn(self, role, argv, *, stdin=None, stdout=None, stderr=None):
        """Only callers with completed admission may supply production argv."""
        self._held()
        if role not in ('storage', 'native') or role in self.children:
            raise LifetimeRefused('role_already_used_or_invalid')
        if not isinstance(argv, (list, tuple)) or not argv or not all(isinstance(x, str) for x in argv):
            raise LifetimeRefused('argv')
        process = subprocess.Popen(argv, stdin=stdin, stdout=stdout, stderr=stderr,
                                   close_fds=True, pass_fds=(self.lease.fd,))
        self.children[role] = process
        return process

    def wait(self, role, timeout=None):
        self._held()
        return self.children[role].wait(timeout=timeout)

    def quit_storage(self, connection, timeout=5):
        self._held()
        storage = self.children.get('storage')
        if storage is None:
            raise LifetimeRefused('storage_not_tracked')

        def idle():
            self._held()
            return storage.poll() is None and all(
                role == 'storage' or process.poll() is not None
                for role, process in self.children.items())

        if not idle():
            raise LifetimeRefused('native_active_or_storage_exited')
        return acknowledged_quit(connection, storage.pid, idle, timeout)

    def release_after_idle(self):
        self._held()
        if any(process.poll() is None for process in self.children.values()):
            raise LifetimeRefused('child_active')
        for line in pathlib.Path('/proc/self/mountinfo').read_text().splitlines():
            encoded = line.split()[4]
            path = re.sub(r'\\([0-7]{3})', lambda m: chr(int(m[1], 8)), encoded)
            if path == self.mountpoint:
                raise LifetimeRefused('mount_still_present')
        self.lease.__exit__()
        return {'lease_released': True, 'tracked_children_exited': True,
                'mount_absent': True, 'production_admission': False}
