
import sys,os,pathlib,json,time,subprocess,hashlib,fcntl
P=pathlib.Path;R=P(sys.argv[1]);C=R/'control';sys.path.insert(0,str(C))
sys.excepthook=lambda t,v,b:print(json.dumps({'state':'native_queue_requires_inspection','error_type':t.__name__,'details_suppressed':True}))
from archive_recovery_io_admission import CalmWindow
O=P('/home/universe-restore/doge-bounded-native-recovery-v1-20261019.json');gate=CalmWindow()
lock=os.open(C/'queue-owner.lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600);fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
def publish(d):
 d.update(checked_unix=time.time(),queue_pid=os.getpid(),unit='universe-ord-dogecoin-bounded-native-recovery-v1-20261019.service',production_application_ready=False)
 temp=O.with_suffix('.tmp');temp.write_text(json.dumps(d,indent=2));os.replace(temp,O)
assert P('/etc/machine-id').read_text().strip()=='9c42ad634bda4b0ab82b61c4091c7a37'
while True:
 psi={}
 for line in P('/proc/pressure/io').read_text().splitlines():
  f=line.split();psi[f[0]]=float(dict(x.split('=',1) for x in f[1:])['avg10'])
 memory=dict(x.split(':',1) for x in P('/proc/meminfo').read_text().splitlines());v=os.statvfs('/var/lib/universe-ord-dogecoin-archive')
 sample=dict(checked_unix=time.time(),io_full_avg10=psi['full'],io_some_avg10=psi['some'],memory_available_bytes=int(memory['MemAvailable'].split()[0])*1024,free_host_bytes=v.f_bavail*v.f_frsize)
 ready=gate.observe(sample,time.time());publish(dict(state='waiting_for_conservative_resource_admission',resource_sample=sample,calm_samples=len(gate.samples),required_calm_samples=3,native_database_opens=0,native_progress_percent=0))
 if ready:break
 time.sleep(10)
# Exactly one owned attempt after launch gate. Failure never retries.
helper=P('/home/ubuntu/doge-successor-readonly-rpc-v1-20261019.py');assert hashlib.sha256(helper.read_bytes()).hexdigest()=='0dc8069a2636aa646809a79d401d29a428d4c83d8612b1630506eff0fa343434'
q=subprocess.run([sys.executable,'-B',str(helper)],capture_output=True,text=True,timeout=30);assert q.returncode==0;rpc=json.loads(q.stdout);routes=rpc['routes']
net=subprocess.run(['ss','-tnpH'],capture_output=True,text=True,timeout=8);assert net.returncode==0
peer=any('51.222.108.117:22' in x and 'pid=2227042,' in x and x.split()[0]=='ESTAB' for x in net.stdout.splitlines());assert peer
assert rpc['new_tunnel_and_redirect_agree'] is True and all(x['authenticated_read_passed'] and x['mainnet_genesis_verified'] and not x['ibd'] for x in routes)
(C/'rpc-admission.private.json').write_text(json.dumps(dict(checked_unix=rpc['checked_unix'],owned_peer_verified=peer,routes_agree=True,ibd=False,genesis_verified=True,blocks=routes[0]['blocks'])))
stdout=(C/'job.stdout.private').open('xb');stderr=(C/'job.stderr.private').open('xb')
child=subprocess.Popen(['unshare','--mount','--net','--propagation','private','--',sys.executable,'-B',str(C/'native-job.py'),str(R)],stdout=stdout,stderr=stderr,pass_fds=(lock,))
while child.poll() is None:
 phase=C/'phase.private.json';d=json.loads(phase.read_text()) if phase.exists() else dict(state='one_native_attempt_preparing',native_database_opens=0)
 d['namespace_pid']=child.pid;publish(d);time.sleep(10)
stdout.close();stderr.close()
try:result=json.loads((C/'job.stdout.private').read_text())
except (ValueError,TypeError):result=dict(state='native_attempt_requires_inspection_no_retry',details_suppressed=True)
result.update(namespace_exit=child.returncode,automatic_retry=False,application_activation_performed=False)
publish(result)
