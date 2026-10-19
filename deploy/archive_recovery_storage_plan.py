"""Exact storage argv construction, not admission or a production launcher.

Caller must establish the private read-only source mount, pinned identities,
held cooperating lease and fresh capacity policy before executing this plan.
Per-file RLIMIT_FSIZE does not reserve shared filesystem space.
"""
import json
import os
import posixpath
import re
import stat


def _worker_export_guard(paths, worker_gid):
    if type(worker_gid) is not int or worker_gid <= 0:
        raise ValueError('worker_group')
    source, delta, export, qmp = paths
    directory = posixpath.dirname(export)
    control = posixpath.join(directory, 'control')
    if any(posixpath.dirname(p) != control for p in (source, delta, qmp)):
        raise ValueError('private_control_paths')
    for path, gid, mode in [(directory, worker_gid, 0o710), (control, 0, 0o700)]:
        s = os.lstat(path)
        if not stat.S_ISDIR(s.st_mode) or s.st_uid != 0 or s.st_gid != gid or stat.S_IMODE(s.st_mode) != mode:
            raise ValueError('private_export_permissions')
    parent = posixpath.dirname(directory)
    while True:
        s = os.lstat(parent)
        if not stat.S_ISDIR(s.st_mode) or s.st_uid != 0 or (s.st_mode & 0o022 and not s.st_mode & stat.S_ISVTX):
            raise ValueError('private_export_parent')
        if parent == '/':
            break
        parent = posixpath.dirname(parent)


def storage_argv(source_view, delta, export_view, qmp_socket, max_file_bytes, *, worker_gid=None):
    paths = (source_view, delta, export_view, qmp_socket)
    if (not all(isinstance(p, str) and p.startswith('/')
                and posixpath.normpath(p) == p and not p.startswith('//')
                and not any(c in p for c in (',', '\\', '\n', '\r', '\0'))
                for p in paths) or len(set(paths)) != 4):
        raise ValueError('storage_paths')
    if type(max_file_bytes) is not int or not 262144 <= max_file_bytes <= 600_000_000_000:
        raise ValueError('storage_file_limit')
    if worker_gid is not None:
        _worker_export_guard(paths, worker_gid)
    # JSON block definitions prevent QEMU option injection through filenames.
    graph = [
        {'driver': 'file', 'node-name': 'source-file', 'filename': source_view,
         'read-only': True, 'auto-read-only': False},
        {'driver': 'raw', 'node-name': 'source', 'file': 'source-file',
         'read-only': True},
        {'driver': 'file', 'node-name': 'delta-file', 'filename': delta,
         'read-only': False, 'auto-read-only': False},
        {'driver': 'qcow2', 'node-name': 'disk', 'file': 'delta-file',
         'backing': 'source', 'read-only': False},
    ]
    argv = ['/usr/bin/prlimit', '--fsize={0}:{0}'.format(max_file_bytes), '--',
            '/usr/bin/env', '--ignore-signal=XFSZ',
            '/usr/bin/setpriv', '--bounding-set=-sys_resource', '--inh-caps=-all',
            '--ambient-caps=-all', '--no-new-privs', '/usr/bin/qemu-storage-daemon',
            '--chardev', 'socket,id=qmp,path='+qmp_socket+',server=on,wait=off',
            '--monitor', 'chardev=qmp']
    for definition in graph:
        argv.extend(['--blockdev', json.dumps(definition, separators=(',', ':'))])
    argv.extend(['--export', 'type=fuse,id=archive-export,node-name=disk,mountpoint='
                 +export_view+',writable=on,growable=on,allow-other='
                 +('on' if worker_gid is not None else 'off')])
    return argv


def _fuse_mount(path):
    matches = []
    with open('/proc/self/mountinfo') as stream:
        for line in stream:
            fields = line.split()
            mountpoint = re.sub(r'\\([0-7]{3})', lambda m: chr(int(m[1], 8)), fields[4])
            if mountpoint == path:
                filesystem = fields[fields.index('-')+1]
                if filesystem == 'fuse' or filesystem.startswith('fuse.'):
                    matches.append((fields[0], fields[2], filesystem))
    if len(matches) != 1:
        raise ValueError('exact_fuse_mount_required')
    return matches[0]


def prepare_worker_export(source_view, delta, export_view, qmp_socket, worker_gid, native_idle):
    """Set ephemeral FUSE view permissions before native admission, never backing files.

    allow-other alone leaves QEMU 8.2 exports root:root 0600. Qualify the mounted
    FUSE descriptor and namespace mount identity before changing its virtual
    attributes. Caller must keep its owner lease and exact daemon qualification.
    """
    if not callable(native_idle) or native_idle() is not True:
        raise ValueError('native_not_idle')
    _worker_export_guard((source_view, delta, export_view, qmp_socket), worker_gid)
    mount = _fuse_mount(export_view)
    fd = os.open(export_view, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        s = os.fstat(fd)
        device = '{}:{}'.format(os.major(s.st_dev), os.minor(s.st_dev))
        if (not stat.S_ISREG(s.st_mode) or s.st_nlink != 1 or s.st_uid != 0
                or device != mount[1] or _fuse_mount(export_view) != mount
                or native_idle() is not True):
            raise ValueError('fuse_descriptor_or_owner_changed')
        os.fchown(fd, 0, worker_gid)
        os.fchmod(fd, 0o660)
        s = os.fstat(fd)
        if s.st_uid != 0 or s.st_gid != worker_gid or stat.S_IMODE(s.st_mode) != 0o660:
            raise ValueError('fuse_permissions_not_applied')
        return {'worker_export_permissions_verified': True,
                'backing_file_permissions_changed': False, 'production_admission': False}
    finally:
        os.close(fd)
