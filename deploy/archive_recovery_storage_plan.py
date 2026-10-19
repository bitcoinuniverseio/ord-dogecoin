"""Exact storage argv construction, not admission or a production launcher.

Caller must establish the private read-only source mount, pinned identities,
held cooperating lease and fresh capacity policy before executing this plan.
Per-file RLIMIT_FSIZE does not reserve shared filesystem space.
"""
import json
import posixpath


def storage_argv(source_view, delta, export_view, qmp_socket, max_file_bytes):
    paths = (source_view, delta, export_view, qmp_socket)
    if (not all(isinstance(p, str) and p.startswith('/')
                and posixpath.normpath(p) == p and not p.startswith('//')
                and not any(c in p for c in (',', '\\', '\n', '\r', '\0'))
                for p in paths) or len(set(paths)) != 4):
        raise ValueError('storage_paths')
    if type(max_file_bytes) is not int or not 262144 <= max_file_bytes <= 600_000_000_000:
        raise ValueError('storage_file_limit')
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
                 +export_view+',writable=on,growable=on,allow-other=off'])
    return argv
