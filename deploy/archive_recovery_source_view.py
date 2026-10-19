"""Lease-bound private read-only source mount, no production launcher.

Caller enters a private mount namespace and makes propagation private first.
No automatic teardown or child signals occur on failure. This is one storage
supervisor component; capacity, external owners and native admission are separate.
"""
import os
import pathlib
import re
import stat
import subprocess

from archive_recovery_lifetime import LifetimeRefused


def mounts():
    rows = []
    for line in pathlib.Path('/proc/self/mountinfo').read_text().splitlines():
        f = line.split(); separator = f.index('-')
        path = re.sub(r'\\([0-7]{3})', lambda m: chr(int(m[1], 8)), f[4])
        rows.append((path, set(f[5].split(',')), f[6:separator], f[0]))
    return rows


def private_namespace():
    if (os.geteuid() != 0 or os.stat('/proc/self/ns/mnt').st_ino == os.stat('/proc/1/ns/mnt').st_ino
            or any(x.startswith(('shared:', 'master:')) for row in mounts() for x in row[2])):
        raise LifetimeRefused('private_mount_namespace_required')


def guarded_file(path):
    path = os.fspath(path)
    if not path.startswith('/') or os.path.normpath(path) != path:
        raise LifetimeRefused('source_view_path')
    s = os.lstat(path)
    if not stat.S_ISREG(s.st_mode) or s.st_uid != 0 or s.st_nlink != 1 or s.st_mode & 0o022:
        raise LifetimeRefused('source_view_file')
    for parent in pathlib.Path(path).parents:
        p = parent.lstat()
        if (not stat.S_ISDIR(p.st_mode) or p.st_uid != 0
                or (p.st_mode & 0o022 and not p.st_mode & stat.S_ISVTX)):
            raise LifetimeRefused('source_view_parent')
    return s


def _mount(argv):
    result = subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if result.returncode:
        raise LifetimeRefused('source_mount_operation_failed')


class ReadonlySourceView:
    def __init__(self, group, source, view):
        private_namespace(); group._held()
        self.group, self.source, self.view = group, os.fspath(source), os.fspath(view)
        source_stat, view_stat = guarded_file(source), guarded_file(view)
        if (source_stat.st_dev, source_stat.st_ino) == (view_stat.st_dev, view_stat.st_ino):
            raise LifetimeRefused('source_view_alias')
        if any(r[0] == self.view for r in mounts()):
            raise LifetimeRefused('source_view_already_mounted')
        self.identity = (source_stat.st_dev, source_stat.st_ino, source_stat.st_size)
        self.mount_id = None
        # Register BEFORE mount: even a failed remount cannot release the lease
        # while the partially established view remains mounted.
        group.protect_mount(self.view)
        _mount(['/usr/bin/mount', '--bind', self.source, self.view])
        _mount(['/usr/bin/mount', '-o', 'remount,bind,ro,nosuid,nodev,noexec', self.view])
        self.verify()

    def verify(self):
        private_namespace(); self.group._held()
        s = guarded_file(self.view)
        rows = [r for r in mounts() if r[0] == self.view]
        if (len(rows) != 1 or not {'ro', 'nosuid', 'nodev', 'noexec'} <= rows[0][1]
                or (s.st_dev, s.st_ino, s.st_size) != self.identity
                or (self.mount_id is not None and rows[0][3] != self.mount_id)):
            raise LifetimeRefused('source_readonly_mount_changed')
        self.mount_id = rows[0][3]
        return True

    def spawn(self, role, argv, **streams):
        self.verify()
        return self.group.spawn(role, argv, **streams)

    def close_when_idle(self):
        self.verify()
        if any(p.poll() is None for p in self.group.children.values()):
            raise LifetimeRefused('source_view_child_active')
        _mount(['/usr/bin/umount', self.view])
        if any(r[0] == self.view for r in mounts()):
            raise LifetimeRefused('source_view_mount_remains')
        return {'source_view_removed': True, 'production_admission': False}
