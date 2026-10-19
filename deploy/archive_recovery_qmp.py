"""Bounded QMP shutdown transport, not a service lifecycle or authorization tool.

The caller holds its owner lease, connects to its already-qualified daemon,
and supplies an idle_guard that revalidates ownership and native-client exit.
After acknowledgment it must still verify daemon exit and mount removal.
No automatic retry, process signal, service command or database open exists.
"""
import json
import math
import socket
import struct
import time


class QmpRefused(RuntimeError):
    """Fixed diagnostic labels only; peer payloads are never included."""


def _peer_pid(connection):
    if not hasattr(socket, 'SO_PEERCRED'):
        raise QmpRefused('linux_peer_credentials_required')
    return struct.unpack('3i', connection.getsockopt(
        socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize('3i')))[0]


def _is_unix(connection):
    return hasattr(socket, 'AF_UNIX') and connection.family == socket.AF_UNIX


def acknowledged_quit(connection, expected_peer_pid, idle_guard, timeout=5):
    """Use a fresh connected Linux Unix socket; retain it until quit replies.

    idle_guard is trusted caller policy, not an authorization inferred here.
    A timeout or missing reply is an unresolved outcome: never retry or signal
    automatically. The caller retains its lease and observes the exact owner.
    """
    if (type(expected_peer_pid) is not int or expected_peer_pid <= 0
            or type(timeout) not in (int, float) or not math.isfinite(timeout)
            or not 0 < timeout <= 30 or not callable(idle_guard)):
        raise QmpRefused('arguments')
    if not _is_unix(connection):
        raise QmpRefused('unix_socket_required')
    if _peer_pid(connection) != expected_peer_pid:
        raise QmpRefused('peer_identity')
    if idle_guard() is not True:
        raise QmpRefused('owner_not_idle')
    previous_timeout = connection.gettimeout()
    deadline = time.monotonic() + timeout
    buffer = bytearray()

    def budget():
        left = deadline - time.monotonic()
        if left <= 0:
            raise QmpRefused('deadline')
        connection.settimeout(left)

    def receive():
        while b'\n' not in buffer:
            budget()
            chunk = connection.recv(4096)
            if not chunk:
                raise QmpRefused('missing_acknowledgment')
            buffer.extend(chunk)
            if len(buffer) > 65536:
                raise QmpRefused('response_bound')
        line, _, remaining = buffer.partition(b'\n')
        buffer[:] = remaining
        try:
            row = json.loads(line)
        except (ValueError, UnicodeError):
            raise QmpRefused('invalid_response') from None
        if not isinstance(row, dict):
            raise QmpRefused('invalid_response')
        return row

    def command(name, command_id):
        budget()
        connection.sendall(json.dumps({'execute': name, 'id': command_id}).encode() + b'\n')
        for _ in range(64):
            row = receive()
            if 'event' in row and 'id' not in row:
                continue
            if row.get('id') != command_id or 'error' in row or 'return' not in row:
                raise QmpRefused('command_not_acknowledged')
            return
        raise QmpRefused('event_bound')

    try:
        if not isinstance(receive().get('QMP'), dict):
            raise QmpRefused('greeting')
        command('qmp_capabilities', 'archive-capabilities')
        # Never issue quit solely because an earlier snapshot said idle.
        if _peer_pid(connection) != expected_peer_pid or idle_guard() is not True:
            raise QmpRefused('owner_changed')
        command('quit', 'archive-quit')
        return {'quit_acknowledged': True, 'daemon_exit_verified': False,
                'mount_removal_verified': False}
    except (OSError, TimeoutError):
        raise QmpRefused('transport_or_deadline') from None
    finally:
        connection.settimeout(previous_timeout)
