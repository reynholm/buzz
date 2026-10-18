//! Hosted Windows runtime probes: real ACP spawn and independent process observation.
use super::*;
use std::path::Path;
use std::time::{Duration, Instant};

// A nested detached process outlives both its launcher and the adapter. Its
// bounded lifetime and the fixture's taskkill guard also clean up a RED run.
const ADAPTER: &str = r#"
import ctypes,json,os,subprocess,sys,time
mode,marker=sys.argv[1:]
if mode=='leaf':
    time.sleep(45)
    sys.exit(0)
if mode=='middle':
    child=subprocess.Popen([sys.executable,'-u',__file__,'leaf',marker],
        creationflags=0x00000008|0x00000200,stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    open(marker,'w').write(json.dumps({'descendant':child.pid}))
    sys.exit(0)
data={'adapter':os.getpid(),'console':int(ctypes.windll.kernel32.GetConsoleWindow())}
if mode=='detached':
    middle=subprocess.Popen([sys.executable,'-u',__file__,'middle',marker],
        stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    middle.wait(timeout=5)
    data.update(json.load(open(marker)))
if mode=='breakaway':
    try:
        child=subprocess.Popen([sys.executable,'-u',__file__,'leaf',marker],
            creationflags=0x01000000|0x00000008,stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        data['descendant']=child.pid
    except OSError as error:
        data['refused']=error.winerror
open(marker,'w').write(json.dumps(data))
for line in sys.stdin:
    request=json.loads(line)
    if 'id' in request:
        print(json.dumps({'jsonrpc':'2.0','id':request['id'],
            'result':{'protocolVersion':2,'agentCapabilities':{}}}),flush=True)
if mode=='ignore':
    time.sleep(45)
"#;

struct Probe {
    client: Option<AcpClient>,
    data: serde_json::Value,
    _dir: tempfile::TempDir,
}

impl Probe {
    async fn new(mode: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("adapter.py");
        let marker = dir.path().join("ready.json");
        std::fs::write(&script, ADAPTER).unwrap();
        let mut client = AcpClient::spawn(
            "python",
            &[
                "-u".into(),
                script.to_string_lossy().into_owned(),
                mode.into(),
                marker.to_string_lossy().into_owned(),
            ],
            &[],
            false,
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(10), client.initialize())
            .await
            .unwrap()
            .unwrap();
        let data = read_marker(&marker);
        assert_eq!(data["adapter"].as_u64(), client.process_id().map(u64::from));
        Self {
            client: Some(client),
            data,
            _dir: dir,
        }
    }

    fn client(&mut self) -> &mut AcpClient {
        self.client.as_mut().unwrap()
    }

    fn pid(&self, key: &str) -> u32 {
        self.data[key].as_u64().unwrap().try_into().unwrap()
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        // Own only these fixture PIDs; never a broad name-based cleanup.
        for key in ["descendant", "adapter"] {
            if let Some(pid) = self.data[key].as_u64() {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    }
}

fn read_marker(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn alive(pid: u32) -> bool {
    // Ask Windows independently of process-wrap/job completion notifications.
    let output = std::process::Command::new("python")
        .args([
            "-c",
            r#"
import ctypes,sys
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.restype=ctypes.c_void_p
k.WaitForSingleObject.argtypes=[ctypes.c_void_p,ctypes.c_uint32]
k.CloseHandle.argtypes=[ctypes.c_void_p]
h=k.OpenProcess(0x00100000,False,int(sys.argv[1]))
if not h:
    assert ctypes.get_last_error()==87,ctypes.get_last_error()
    print('dead')
else:
    result=k.WaitForSingleObject(h,0)
    k.CloseHandle(h)
    assert result in (0,258),result
    print('alive' if result==258 else 'dead')
"#,
            &pid.to_string(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    match String::from_utf8(output.stdout).unwrap().trim() {
        "alive" => true,
        "dead" => false,
        other => panic!("unexpected process probe: {other}"),
    }
}

async fn assert_dead(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(pid) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(!alive(pid), "fixture process {pid} survived containment");
}

#[tokio::test]
async fn checked_termination_windows_adapter_exits_on_eof() {
    let mut probe = Probe::new("eof").await;
    let pid = probe.pid("adapter");
    let started = Instant::now();
    assert_eq!(probe.client().terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_dead(pid).await;
}

#[tokio::test]
async fn checked_termination_windows_ignores_eof_until_fifteen_seconds() {
    let mut probe = Probe::new("ignore").await;
    let pid = probe.pid("adapter");
    let started = Instant::now();
    assert_eq!(probe.client().terminate().await, TerminationReport::Clean);
    assert!(started.elapsed() >= Duration::from_secs(15));
    assert!(started.elapsed() < Duration::from_secs(25));
    assert_dead(pid).await;
}

#[tokio::test]
async fn checked_termination_windows_detached_grandchild_blocks_clean_after_parent_exit() {
    let mut probe = Probe::new("detached").await;
    let parent = probe.pid("adapter");
    let child = probe.pid("descendant");
    assert!(alive(child));
    // Cancellation/reentry must not turn a cached parent exit into job proof.
    assert!(
        tokio::time::timeout(Duration::from_secs(5), probe.client().terminate())
            .await
            .is_err()
    );
    assert_dead(parent).await;
    assert!(
        alive(child),
        "the detached grandchild must outlive the adapter"
    );
    let started = Instant::now();
    let report = tokio::time::timeout(Duration::from_secs(12), probe.client().terminate())
        .await
        .expect("reentry restarted the graceful deadline");
    assert_eq!(report, TerminationReport::Clean);
    assert!(started.elapsed() >= Duration::from_secs(8));
    assert!(started.elapsed() < Duration::from_secs(12));
    assert_dead(child).await;
    // A second explicit attempt must retain actual zero-job proof.
    assert_eq!(probe.client().terminate().await, TerminationReport::Clean);
}

#[tokio::test]
async fn checked_termination_windows_breakaway_is_refused_or_contained() {
    let mut probe = Probe::new("breakaway").await;
    if let Some(error) = probe.data["refused"].as_u64() {
        assert_eq!(error, 5, "expected job breakaway access denial");
    } else {
        assert!(alive(probe.pid("descendant")));
    }
    assert_eq!(probe.client().terminate().await, TerminationReport::Clean);
    if probe.data["descendant"].is_number() {
        assert_dead(probe.pid("descendant")).await;
    }
}

#[tokio::test]
async fn checked_termination_windows_preserves_create_no_window() {
    let mut probe = Probe::new("eof").await;
    assert_eq!(
        probe.data["console"], 0,
        "adapter acquired a console window"
    );
    assert_eq!(probe.client().terminate().await, TerminationReport::Clean);
}

#[tokio::test]
async fn checked_termination_windows_harness_drop_kills_detached_tree() {
    let mut probe = Probe::new("detached").await;
    let parent = probe.pid("adapter");
    let child = probe.pid("descendant");
    assert!(alive(parent));
    assert!(alive(child));
    drop(probe.client.take());
    assert_dead(parent).await;
    assert_dead(child).await;
}

#[tokio::test]
async fn checked_termination_windows_query_error_after_parent_reap_is_unverified() {
    let mut probe = Probe::new("detached").await;
    let parent = probe.pid("adapter");
    let descendant = probe.pid("descendant");
    probe.client().stdin.take();
    assert_dead(parent).await;
    assert!(matches!(probe.client().child.try_wait(), Ok(Some(_))));
    assert!(alive(descendant));
    assert_eq!(
        probe
            .client()
            .terminate_windows_with_query(|_| {
                Err(std::io::Error::other("fixture job query failed"))
            })
            .await,
        TerminationReport::Unverified
    );
    assert!(
        alive(descendant),
        "query failure is not proof of stopped work"
    );
    drop(probe.client.take());
    assert_dead(descendant).await;
}

#[tokio::test]
async fn checked_termination_windows_empty_query_requires_parent_reap() {
    let mut probe = Probe::new("ignore").await;
    let parent = probe.pid("adapter");
    // An empty query alone cannot authorize replacement of a live parent.
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            probe.client().terminate_windows_with_query(|_| Ok(0))
        )
        .await
        .is_err(),
        "empty job query accepted an unreaped parent"
    );
    assert!(alive(parent));
    drop(probe.client.take());
    assert_dead(parent).await;
}

#[tokio::test]
async fn checked_termination_windows_verification_deadline_survives_cancellation() {
    let mut probe = Probe::new("ignore").await;
    let parent = probe.pid("adapter");
    let started = Instant::now();
    // Hold the queried job nonempty after the real kill to exercise the entire
    // verification window, including cancellation five seconds into that phase.
    assert!(tokio::time::timeout(
        Duration::from_secs(20),
        probe.client().terminate_windows_with_query(|_| Ok(1))
    )
    .await
    .is_err());
    assert_dead(parent).await;
    assert!(matches!(probe.client().child.try_wait(), Ok(Some(_))));
    let resumed = Instant::now();
    let report = tokio::time::timeout(
        Duration::from_secs(6),
        probe.client().terminate_windows_with_query(|_| Ok(1)),
    )
    .await
    .expect("reentry restarted a retirement deadline");
    assert_eq!(report, TerminationReport::Leftover { count: 1 });
    assert!(resumed.elapsed() < Duration::from_secs(6));
    assert!(started.elapsed() >= Duration::from_secs(25));
    assert!(started.elapsed() < Duration::from_secs(27));
    // The real empty query and reaped parent are required even on a later retry.
    assert_eq!(probe.client().terminate().await, TerminationReport::Clean);
}
