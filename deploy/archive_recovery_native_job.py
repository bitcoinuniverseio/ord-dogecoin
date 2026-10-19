
import sys,os,pathlib,json,time,subprocess,hashlib,stat,socket,errno
sys.excepthook=lambda t,v,b:print(json.dumps({'state':'bounded_storage_namespace_requires_inspection','error_type':t.__name__,'details_suppressed':True}))
P=pathlib.Path;R=P(sys.argv[1]);C=R/'control';sys.path.insert(0,str(C))
from archive_recovery_lifetime import LeaseGroup
from archive_recovery_source_view import ReadonlySourceView
from archive_recovery_storage_plan import storage_argv,prepare_worker_export
from archive_recovery_identity import inspect_identity,assess_identity,worker_argv
ROOT=P('/var/lib/universe-ord-dogecoin-archive');container=ROOT/'capacity-v1-20261019/container.ext4';volume=C/'bounded';volume.mkdir(mode=0o700)
base=P('/home/universe-restored/ord-dogecoin-archive/561b9bca/source/doginals.redb');original_delta=ROOT/'changes.qcow2'
qualified=json.loads(P('/home/universe-restore/doge-bounded-storage-qualification-v1-20261019.json').read_text());copy={'container_identity':qualified['container_identity'],'working_delta_sha256':qualified['working_delta_after_storage']['sha256']};pinned=json.loads(P('/home/universe-restore/doge-bounded-container-identity-v1-20261019.json').read_text())
def ident(p):
 s=p.lstat();return [s.st_dev,s.st_ino,s.st_size,s.st_mtime_ns,s.st_ctime_ns,s.st_uid,s.st_gid,stat.S_IMODE(s.st_mode),s.st_nlink]
def run(args):
 q=subprocess.run(args,capture_output=True,text=True);assert q.returncode==0;return q
def mounted(path):return any(x.split()[4]==str(path) for x in P('/proc/self/mountinfo').read_text().splitlines())
def idle():
 for u in ['universe-ord-dogecoin-mainnet.service','universe-ord-dogecoin-archive-storage.service']:
  q=run(['systemctl','show',u,'--property=MainPID','--value']);assert q.stdout.strip()=='0'
 assert not mounted(ROOT/'doginals.redb')
def checkpoint(state,**fields):
 out=dict(checked_unix=time.time(),state=state,production_admission=False,native_database_opens=0);out.update(fields);temp=C/'phase.private.tmp';temp.write_text(json.dumps(out));os.replace(temp,C/'phase.private.json')

from archive_recovery_io_admission import calm
def preflight():
 pressure={}
 for line in P('/proc/pressure/io').read_text().splitlines():
  f=line.split();pressure[f[0]]=float(dict(x.split('=',1) for x in f[1:])['avg10'])
 memory=dict(x.split(':',1) for x in P('/proc/meminfo').read_text().splitlines());v=os.statvfs(ROOT)
 resources=dict(checked_unix=time.time(),io_full_avg10=pressure['full'],io_some_avg10=pressure['some'],memory_available_bytes=int(memory['MemAvailable'].split()[0])*1024,free_host_bytes=v.f_bavail*v.f_frsize)
 assert calm(resources,time.time())
 targets={(p.stat().st_dev,p.stat().st_ino) for p in [base,original_delta,container]};mapped={(os.major(d),os.minor(d),i) for d,i in targets};fd_matches=map_matches=denied=0
 for process in P('/proc').iterdir():
  if not process.name.isdigit():continue
  try:
   for f in (process/'fd').iterdir():
    try:
     s=f.stat();fd_matches+=int((s.st_dev,s.st_ino) in targets)
    except FileNotFoundError:pass
   for line in (process/'maps').read_text().splitlines():
    f=line.split()
    if len(f)<5:continue
    major,minor=(int(x,16) for x in f[3].split(':'));map_matches+=int((major,minor,int(f[4])) in mapped)
  except (FileNotFoundError,ProcessLookupError):pass
  except PermissionError:denied+=1
 assert fd_matches==map_matches==denied==0
 helper=P('/home/ubuntu/doge-successor-readonly-rpc-v1-20261019.py');assert hashlib.sha256(helper.read_bytes()).hexdigest()=='0dc8069a2636aa646809a79d401d29a428d4c83d8612b1630506eff0fa343434'
 # The queue collects authenticated RPC in the host network before unshare.
 rpc=json.loads((C/'rpc-admission.private.json').read_text());assert -1<=time.time()-rpc['checked_unix']<=120 and rpc['owned_peer_verified'] is True
 assert rpc['routes_agree'] is True and rpc['ibd'] is False and rpc['genesis_verified'] is True
 return dict(resource_sample=resources,target_fd_matches=0,target_mapped_matches=0,permission_denials=0,rpc_blocks=rpc['blocks'],rpc_checked_unix=rpc['checked_unix'])

idle();before=[ident(base),ident(original_delta)];retained=json.loads(P('/home/universe-restore/validation-tools/doge-successor-live-collector-v2-20261019/snapshot.private.json').read_text())['manifest'];identity_fields=['dev','inode','bytes','mtime_ns','ctime_ns','uid','gid','mode','links'];assert before==[[retained[k][f] for f in identity_fields] for k in ['base_identity','delta_identity']];assert hashlib.sha256(original_delta.read_bytes()).hexdigest()==retained['delta_sha256'];assert ident(container)==copy['container_identity']
assert not run(['/usr/sbin/losetup','-j',str(container)]).stdout.strip()
v=os.statvfs(ROOT);assert v.f_bavail*v.f_frsize>=300_000_000_000+256*1024**3
assert assess_identity(inspect_identity(),time.time())['identity_qualified']
group=LeaseGroup(ROOT/'recovery-successor-owner.lock',R/'view.redb');admission=preflight();group.protect_mount(str(volume))
header=run(['/usr/sbin/dumpe2fs','-h',str(container)]).stdout;fields=dict(x.split(':',1) for x in header.splitlines() if ':' in x);assert fields['Filesystem UUID'].strip()==pinned['filesystem_uuid'] and fields['Filesystem state'].strip()=='clean'
checkpoint('mounting_existing_bounded_container')
run(['/usr/bin/mount','-o','loop,nosuid,nodev,noexec',str(container),str(volume)])
mount_rows=[x.split() for x in P('/proc/self/mountinfo').read_text().splitlines() if x.split()[4]==str(volume)];assert len(mount_rows)==1 and {'rw','nosuid','nodev','noexec'}<=set(mount_rows[0][5].split(','))
assert (P('/sys/dev/block')/mount_rows[0][2]/'loop/backing_file').read_text().strip()==str(container)
working=volume/'changes.qcow2';w=working.stat();assert w.st_ino==12 and w.st_size==393216 and hashlib.sha256(working.read_bytes()).hexdigest()==copy['working_delta_sha256']
source=C/'source.raw';delta=C/'delta.qcow2';export=R/'view.redb';qmp=C/'qmp.sock'
for p in (source,delta,export):p.touch(mode=0o600)
group.protect_mount(str(delta));run(['/usr/bin/mount','--bind',str(working),str(delta)]);assert (delta.stat().st_dev,delta.stat().st_ino)==(w.st_dev,w.st_ino)
readonly=ReadonlySourceView(group,str(base),str(source));assert readonly.verify()
log=(C/'qsd.private').open('xb');checkpoint('starting_exact_bounded_storage')
storage=readonly.spawn('storage',storage_argv(str(source),str(delta),str(export),str(qmp),8589934592,worker_gid=977),stdout=log,stderr=log)
checkpoint('waiting_owned_storage_export',storage_pid=storage.pid)
for _ in range(200):
 if mounted(export) or storage.poll() is not None:break
 time.sleep(.03)
assert storage.poll() is None and mounted(export)
status=dict(x.split(':',1) for x in (P('/proc')/str(storage.pid)/'status').read_text().splitlines() if ':' in x)
assert status['NoNewPrivs'].strip()=='1' and not int(status['CapBnd'].strip(),16)&(1<<24)
line=next(x for x in (P('/proc')/str(storage.pid)/'limits').read_text().splitlines() if x.startswith('Max file size'));assert line.split()[3:5]==['8589934592','8589934592']
rofd=rwfd=False;bs=source.stat();ds=delta.stat()
for f in (P('/proc')/str(storage.pid)/'fd').iterdir():
 try:
  s=f.stat();info=dict(x.split(':',1) for x in (P('/proc')/str(storage.pid)/'fdinfo'/f.name).read_text().splitlines() if ':' in x);mode=int(info['flags'].strip(),8)&os.O_ACCMODE
  if (s.st_dev,s.st_ino)==(bs.st_dev,bs.st_ino):rofd=mode==os.O_RDONLY
  if (s.st_dev,s.st_ino)==(ds.st_dev,ds.st_ino):rwfd=mode==os.O_RDWR
 except OSError:pass
assert rofd and rwfd
prepare_worker_export(str(source),str(delta),str(export),str(qmp),977,lambda:all(role=='storage' or p.poll() is not None for role,p in group.children.items()))

scanner=R/'scanner';assert hashlib.sha256(scanner.read_bytes()).hexdigest()=='6711b8f9649a543900a2f8639d147beda9f6f2683a1596b32a1a2381f31df779'
fd=os.open(export,os.O_RDWR|os.O_NOFOLLOW|os.O_CLOEXEC);s=os.fstat(fd);assert s.st_size==2258359144448 and s.st_nlink==1 and s.st_gid==977
output=(C/'native.stdout.private').open('xb');error=(C/'native.stderr.private').open('xb')
native=readonly.spawn('native',worker_argv(str(scanner),['recover-fd',str(fd),str(s.st_dev),str(s.st_ino),str(s.st_size)]),stdout=output,stderr=error,native_fd=fd)
checkpoint('owned_native_recovery_running_no_deadline',storage_pid=storage.pid,native_pid=native.pid,native_database_opens=1,admission=admission)
# No timeout, signal, kill, retry, reindex or reset. Natural exit only.
native_exit=group.wait('native');os.close(fd);output.close();error.close()
native_stdout=(C/'native.stdout.private').read_text();native_stderr=(C/'native.stderr.private').read_text()
import re
native_result=dict(exit=native_exit,clean_integrity_reported='integrity_clean' in native_stdout,recovery_reported='open_recovery_completed_integrity_clean_requires_inspection' in native_stderr,io_error_reported='category=io' in native_stderr,errno_codes=[int(x) for x in re.findall(r'\bos_code=(\d+)\b',native_stderr)])
access=dict(uid=981,gid=977,identity_reaudited_before_launch=True,native_descriptor_passed=True)
checkpoint('native_natural_exit_storage_shutdown_pending',storage_pid=storage.pid,native_pid=native.pid,native_database_opens=1,native_result=native_result)

conn=socket.socket(socket.AF_UNIX);conn.connect(str(qmp))
with conn:ack=group.quit_storage(conn,5)
assert group.wait('storage',10)==0;log.close();assert not mounted(export)
check=run(['/usr/bin/qemu-img','check','-f','qcow2','--output=json',str(delta)]);metadata=json.loads(check.stdout);assert metadata.get('check-errors',0)==0 and metadata.get('corruptions',0)==0
after_delta=dict(identity=ident(working),sha256=hashlib.sha256(working.read_bytes()).hexdigest() if working.stat().st_size<=16*1024**2 else None,digest_omitted_for_large_delta=working.stat().st_size>16*1024**2)
run(['/usr/bin/umount',str(delta)]);readonly.close_when_idle();run(['/usr/bin/umount',str(volume)])
for _ in range(100):
 if not run(['/usr/sbin/losetup','-j',str(container)]).stdout.strip():break
 time.sleep(.05)
assert not run(['/usr/sbin/losetup','-j',str(container)]).stdout.strip();release=group.release_after_idle();assert before==[ident(base),ident(original_delta)];idle()
out=dict(checked_unix=time.time(),state='native_attempt_finished_storage_cleanly_shutdown_inspection_required',native_result=native_result,admission=admission,storage_pid=storage.pid,storage_exit=0,source_fd_readonly=rofd,working_delta_fd_readwrite=rwfd,source_mount_readonly_verified=True,hard_file_limit_bytes=8589934592,container_bytes=container.stat().st_size,container_identity=ident(container),filesystem_uuid_sha256=pinned['filesystem_uuid_sha256'],working_delta_after_storage=after_delta,dedicated_access=access,qmp_acknowledged=ack['quit_acknowledged'],metadata_check_exit=check.returncode,metadata_errors=metadata.get('check-errors',0),corruptions=metadata.get('corruptions',0),all_protected_mounts_removed=True,loop_detached=True,lease_released=release['lease_released'],original_source_and_delta_metadata_unchanged=True,allocation_repeated=False,copy_repeated=False,native_database_opens=1,production_application_service_actions=0,production_admission=False,host_reserve_globally_enforced=False)
checkpoint('complete_native_attempt_no_automatic_retry',native_database_opens=1,native_result=native_result)
print(json.dumps(out))
