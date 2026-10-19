import io
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time
import unittest

from archive_recovery_admission import OwnerLease
from archive_recovery_lifetime import LeaseGroup, LifetimeRefused


@unittest.skipUnless(sys.platform == 'linux', 'Linux cooperating-owner lifetime')
class LifetimeTests(unittest.TestCase):
    def test_live_child_blocks_release_and_wait_does_not_kill(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            group = LeaseGroup(root/'owner.lock', root/'view')
            child = group.spawn('native', [sys.executable, '-c', 'import sys;sys.stdin.buffer.read(1)'], stdin=subprocess.PIPE)
            try:
                with self.assertRaises(LifetimeRefused):
                    group.release_after_idle()
                with self.assertRaises(subprocess.TimeoutExpired):
                    group.wait('native', timeout=.02)
                self.assertIsNone(child.poll())
                with self.assertRaises(BlockingIOError):
                    with OwnerLease(root/'owner.lock'):
                        pass
            finally:
                child.stdin.write(b'x'); child.stdin.close(); group.wait('native', 5)
                group.release_after_idle()

    def test_active_native_prevents_qmp_before_transport(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory); group = LeaseGroup(root/'owner.lock', root/'view')
            children = [group.spawn(role, [sys.executable, '-c', 'import sys;sys.stdin.buffer.read(1)'], stdin=subprocess.PIPE) for role in ['storage', 'native']]
            try:
                with self.assertRaisesRegex(LifetimeRefused, 'native_active'):
                    group.quit_storage(None)
                with self.assertRaises(LifetimeRefused):
                    group.spawn('native', ['unused'])
            finally:
                for child in children:
                    child.stdin.write(b'x'); child.stdin.close(); child.wait(5)
                group.release_after_idle()

    def test_lease_survives_supervisor_exit_while_child_preserves_fd(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory); lock = root/'owner.lock'; release = root/'release'
            child_code = 'import pathlib,time,sys; p=pathlib.Path(sys.argv[1]); stop=time.monotonic()+5\nwhile not p.exists() and time.monotonic()<stop: time.sleep(.01)'
            code = ('import sys,os,subprocess;sys.path.insert(0,sys.argv[1]);'
                    'from archive_recovery_lifetime import LeaseGroup;'
                    'g=LeaseGroup(sys.argv[2],sys.argv[3]);'
                    'p=g.spawn("native",[sys.executable,"-c",sys.argv[4],sys.argv[5]],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);'
                    'print(p.pid,flush=True);os._exit(0)')
            supervisor = subprocess.run([sys.executable, '-B', '-c', code, str(pathlib.Path(__file__).parent), str(lock), str(root/'view'), child_code, str(release)], capture_output=True, text=True, timeout=3)
            self.assertEqual(supervisor.returncode, 0)
            self.assertTrue(supervisor.stdout.strip().isdigit())
            try:
                with self.assertRaises(BlockingIOError):
                    with OwnerLease(lock):
                        pass
            finally:
                release.touch()
            acquired = False
            for _ in range(100):
                try:
                    with OwnerLease(lock):
                        acquired = True
                    break
                except BlockingIOError:
                    time.sleep(.02)
            self.assertTrue(acquired)


if __name__ == '__main__':
    unittest.main()
