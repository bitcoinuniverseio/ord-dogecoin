import json
import contextlib
import os
import socket
import threading
import time
import unittest
from unittest.mock import patch

import archive_recovery_qmp as qmp


class QmpTests(unittest.TestCase):
    def exchange(self, behavior, guard=lambda: True, timeout=.5, peer=None, native=False):
        client, server = socket.socketpair()
        seen, errors = [], []

        def serve():
            try:
                with server, server.makefile('rwb', buffering=0) as stream:
                    def send(row):
                        stream.write(json.dumps(row).encode() + b'\n')
                    send({'QMP': {}})
                    row = json.loads(stream.readline()); seen.append(row['execute'])
                    send({'return': {}, 'id': row['id']})
                    raw = stream.readline()
                    if not raw:
                        return
                    row = json.loads(raw); seen.append(row['execute'])
                    behavior(send, row)
            except (BrokenPipeError, ConnectionResetError):
                pass
            except Exception as error:
                errors.append(type(error).__name__)

        thread = threading.Thread(target=serve, daemon=True); thread.start()
        try:
            with client:
                client.settimeout(2)
                # Protocol tests use socketpair even on Windows without AF_UNIX.
                # The unpatched Linux identity guard has a separate native case.
                identity = contextlib.nullcontext() if native else patch.object(qmp, '_peer_pid', return_value=os.getpid() if peer is None else peer)
                family = contextlib.nullcontext() if native else patch.object(qmp, '_is_unix', return_value=True)
                with family, identity:
                    result = qmp.acknowledged_quit(client, os.getpid(), guard, timeout)
                self.assertEqual(client.gettimeout(), 2)
                return result, seen
        finally:
            thread.join(1)
            self.assertFalse(thread.is_alive())
            self.assertEqual(errors, [])

    def test_ack_after_event_is_required(self):
        def reply(send, row):
            send({'event': 'SHUTDOWN'})
            send({'return': {}, 'id': row['id']})
        result, seen = self.exchange(reply)
        self.assertEqual(seen, ['qmp_capabilities', 'quit'])
        self.assertTrue(result['quit_acknowledged'])
        self.assertFalse(result['daemon_exit_verified'])

    def test_eof_is_not_success(self):
        with self.assertRaisesRegex(qmp.QmpRefused, 'missing_acknowledgment'):
            self.exchange(lambda send, row: None)

    def test_error_payload_is_not_exposed(self):
        with self.assertRaises(qmp.QmpRefused) as error:
            self.exchange(lambda send, row: send({'error': {'desc': 'PRIVATE'}, 'id': row['id']}))
        self.assertNotIn('PRIVATE', str(error.exception))

    def test_changed_idle_guard_prevents_quit(self):
        answers = iter([True, False])
        with self.assertRaisesRegex(qmp.QmpRefused, 'owner_changed'):
            self.exchange(lambda send, row: self.fail('quit sent'), guard=lambda: next(answers))

    def test_wrong_response_id_refused(self):
        with self.assertRaisesRegex(qmp.QmpRefused, 'command_not_acknowledged'):
            self.exchange(lambda send, row: send({'return': {}, 'id': 'other'}))

    def test_no_response_has_bounded_wait(self):
        with self.assertRaisesRegex(qmp.QmpRefused, 'transport_or_deadline|deadline'):
            self.exchange(lambda send, row: time.sleep(.1), timeout=.03)

    def test_oversized_peer_payload_refused(self):
        with self.assertRaisesRegex(qmp.QmpRefused, 'response_bound'):
            self.exchange(lambda send, row: send({'return': 'x'*70000, 'id': row['id']}))

    def test_event_storm_refused(self):
        def events(send, row):
            for _ in range(65):
                send({'event': 'SHUTDOWN'})
        with self.assertRaisesRegex(qmp.QmpRefused, 'event_bound'):
            self.exchange(events)

    @unittest.skipUnless(hasattr(socket, 'SO_PEERCRED'), 'Linux peer credentials')
    def test_actual_linux_identity_and_ack(self):
        result, _ = self.exchange(lambda send, row: send({'return': {}, 'id': row['id']}), native=True)
        self.assertTrue(result['quit_acknowledged'])

    @unittest.skipUnless(hasattr(socket, 'SO_PEERCRED'), 'Linux peer credentials')
    def test_actual_linux_peer_and_wrong_peer(self):
        client, server = socket.socketpair()
        with client, server:
            self.assertTrue(qmp._is_unix(client))
            self.assertEqual(qmp._peer_pid(client), os.getpid())
            with self.assertRaisesRegex(qmp.QmpRefused, 'peer_identity'):
                qmp.acknowledged_quit(client, os.getpid()+1, lambda: True)


if __name__ == '__main__':
    unittest.main()
