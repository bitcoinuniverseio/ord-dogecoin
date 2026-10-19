import os,pathlib,subprocess,sys,tempfile,unittest
from archive_recovery_lifetime import LeaseGroup,LifetimeRefused


@unittest.skipUnless(sys.platform=='linux','Linux inherited descriptor and lease')
class NativeDescriptor(unittest.TestCase):
    def test_native_receives_qualified_descriptor(self):
        with tempfile.TemporaryDirectory() as d:
            r=pathlib.Path(d);p=r/'data';p.write_bytes(b'bounded descriptor')
            fd=os.open(p,os.O_RDWR)
            group=LeaseGroup(r/'owner',r/'view')
            try:
                code='import os,sys;assert os.pread(int(sys.argv[1]),18,0)==b"bounded descriptor"'
                group.spawn('native',[sys.executable,'-c',code,str(fd)],native_fd=fd)
                self.assertEqual(group.wait('native',5),0)
            finally:os.close(fd)
            group.release_after_idle()

    def test_readonly_and_wrong_role_refused_without_launch(self):
        with tempfile.TemporaryDirectory() as d:
            r=pathlib.Path(d);p=r/'data';p.touch();fd=os.open(p,os.O_RDONLY)
            group=LeaseGroup(r/'owner',r/'view')
            try:
                for role in ('native','storage'):
                    with self.assertRaises(LifetimeRefused):group.spawn(role,[sys.executable,'-c','pass'],native_fd=fd)
                self.assertEqual(group.children,{})
            finally:os.close(fd)
            group.release_after_idle()


if __name__=='__main__':unittest.main()
