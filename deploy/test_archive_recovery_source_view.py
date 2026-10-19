import errno
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

from archive_recovery_lifetime import LeaseGroup, LifetimeRefused
from archive_recovery_source_view import ReadonlySourceView, guarded_file


@unittest.skipUnless(sys.platform == 'linux' and os.geteuid() == 0,
                     'root in isolated private Linux mount namespace required')
class SourceView(unittest.TestCase):
    def test_source_mount_and_child_share_lease_until_orderly_exit(self):
        with tempfile.TemporaryDirectory() as d:
            r = pathlib.Path(d); source=r/'base'; view=r/'view'
            source.write_bytes(b'preserved synthetic source');view.touch()
            before=source.stat()
            group=LeaseGroup(r/'owner.lock',r/'export')
            mount=ReadonlySourceView(group,source,view)
            for flags in (os.O_WRONLY,os.O_RDWR):
                with self.assertRaises(OSError) as error:os.close(os.open(view,flags))
                self.assertEqual(error.exception.errno,errno.EROFS)
            with self.assertRaises(LifetimeRefused):group.release_after_idle()
            child=mount.spawn('storage',[sys.executable,'-B','-c','import sys;sys.stdin.buffer.read(1)'],stdin=subprocess.PIPE)
            with self.assertRaises(LifetimeRefused):mount.close_when_idle()
            self.assertIsNone(child.poll());self.assertTrue(mount.verify())
            child.stdin.write(b'x');child.stdin.close();group.wait('storage',5)
            with self.assertRaises(LifetimeRefused):group.release_after_idle()
            mount.close_when_idle();self.assertTrue(group.release_after_idle()['lease_released'])
            after=source.stat();self.assertEqual((before.st_ino,before.st_size,before.st_mtime_ns,before.st_ctime_ns),(after.st_ino,after.st_size,after.st_mtime_ns,after.st_ctime_ns))
            self.assertEqual(source.read_bytes(),b'preserved synthetic source')

    def test_symlink_source_refused_before_mount(self):
        with tempfile.TemporaryDirectory() as d:
            r=pathlib.Path(d);(r/'base').touch();(r/'alias').symlink_to(r/'base')
            with self.assertRaises(LifetimeRefused):guarded_file(r/'alias')


if __name__ == '__main__':unittest.main()
