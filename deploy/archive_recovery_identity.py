"""Pinned I1 archive worker identity audit and argv, no account mutation."""
import math
import os
import pathlib
import time

NAME = 'universe-doge-archive'
UID = 981
GID = 977
MACHINE_ID = '9c42ad634bda4b0ab82b61c4091c7a37'
CHECKS = ('nologin', 'home_absent', 'password_disabled', 'unique_uid',
          'exclusive_primary_group', 'no_supplementary_groups')


def assess_identity(observation, now):
    checked = observation.get('checked_unix')
    fresh = (type(checked) in (int, float) and type(now) in (int, float)
             and math.isfinite(checked) and math.isfinite(now)
             and -5 <= now - checked <= 120)
    valid = (fresh and observation.get('machine_id') == MACHINE_ID
             and observation.get('name') == NAME
             and type(observation.get('uid')) is int and observation['uid'] == UID
             and type(observation.get('gid')) is int and observation['gid'] == GID
             and all(observation.get(k) is True for k in CHECKS))
    return {'identity_qualified': bool(valid), 'production_admission': False}


def inspect_identity():
    """Root-only local audit. Never return or log the shadow field."""
    import pwd
    import grp
    account = pwd.getpwnam(NAME)
    group = grp.getgrgid(account.pw_gid)
    disabled = False
    with open('/etc/shadow', 'rb') as stream:
        for line in stream:
            fields = line.rstrip(b'\n').split(b':')
            if fields[0] == NAME.encode():
                disabled = fields[1].startswith((b'!', b'*'))
                break
    return {
        'checked_unix': time.time(),
        'machine_id': pathlib.Path('/etc/machine-id').read_text().strip(),
        'name': account.pw_name, 'uid': account.pw_uid, 'gid': account.pw_gid,
        'nologin': account.pw_shell == '/usr/sbin/nologin',
        'home_absent': account.pw_dir == '/nonexistent' and not pathlib.Path(account.pw_dir).exists(),
        'password_disabled': disabled,
        'unique_uid': sum(a.pw_uid == account.pw_uid for a in pwd.getpwall()) == 1,
        'exclusive_primary_group': group.gr_name == NAME and not group.gr_mem
            and [a.pw_name for a in pwd.getpwall() if a.pw_gid == account.pw_gid] == [NAME],
        'no_supplementary_groups': not any(NAME in g.gr_mem for g in grp.getgrall()),
    }


def worker_argv(binary, arguments):
    """Reaudit identity before constructing argv; caller still enforces admission."""
    if not isinstance(binary, str) or not os.path.isabs(binary) or not isinstance(arguments, (list, tuple)) or not all(isinstance(a, str) for a in arguments):
        raise ValueError('worker_argv')
    if not assess_identity(inspect_identity(), time.time())['identity_qualified']:
        raise ValueError('worker_identity_changed')
    return ['/usr/bin/setpriv', '--reuid='+str(UID), '--regid='+str(GID),
            '--clear-groups', '--bounding-set=-all', '--inh-caps=-all',
            '--ambient-caps=-all', '--no-new-privs', binary, *arguments]
