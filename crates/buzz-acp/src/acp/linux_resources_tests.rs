//! Opt-in real cgroup checks; invoke explicitly on a user-systemd Linux host.
use super::AcpClient;
use serde_json::json;
use std::{fs, path::PathBuf, process::Command, time::Duration};

const ENV: &str = "BUZZ_ACP_RESOURCE_LIMITS";

struct Fixture {
    slice: String,
    directory: tempfile::TempDir,
    maximum: u64,
}
impl Fixture {
    fn new(maximum: u64) -> Self {
        let slice = format!("buzzguard{}.slice", uuid::Uuid::new_v4().simple());
        let status = Command::new("busctl")
            .args([
                "--user",
                "call",
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1",
                "org.freedesktop.systemd1.Manager",
                "StartTransientUnit",
                "ssa(sv)a(sa(sv))",
                &slice,
                "fail",
                "3",
                "MemoryMax",
                "t",
                &maximum.to_string(),
                "MemoryHigh",
                "t",
                &maximum.to_string(),
                "MemorySwapMax",
                "t",
                "0",
                "0",
            ])
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        Self {
            slice,
            directory: tempfile::tempdir().unwrap(),
            maximum,
        }
    }
    fn command(&self, mode: &str) -> Command {
        let max = if mode == "aggregate" {
            192 * 1024 * 1024
        } else {
            128 * 1024 * 1024
        };
        assert!(max <= self.maximum);
        let policy =
            json!({"slice": self.slice, "memory_high_bytes": max, "memory_max_bytes": max});
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "acp::linux_resources_tests::linux_resources_child",
                "--nocapture",
            ])
            .env(ENV, policy.to_string())
            .env("BUZZ_RESOURCE_TEST_CHILD", mode)
            .env("BUZZ_RESOURCE_TEST_DIR", self.directory.path())
            .env("TMPDIR", self.directory.path())
            .env_remove("BUZZ_ACP_LAUNCH_PREFIX");
        command
    }
    fn run(&self, mode: &str) {
        let result = self.command(mode).output().unwrap();
        assert!(
            result.status.success(),
            "mode={mode}\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new("systemctl")
            .args(["--user", "stop", &self.slice])
            .output();
    }
}

#[test]
#[ignore = "requires a user systemd manager and cgroup v2"]
fn linux_resources_real_boundary() {
    for mode in [
        "boundary",
        "cleanup",
        "invalid",
        "missing_manager",
        "write_race",
    ] {
        Fixture::new(512 * 1024 * 1024).run(mode);
    }
    Fixture::new(256 * 1024 * 1024).run("aggregate");
}

#[tokio::test]
#[ignore = "requires a user systemd manager and cgroup v2"]
async fn linux_resources_parent_exit_retires_scope() {
    let fixture = Fixture::new(512 * 1024 * 1024);
    let mut child = fixture
        .command("parent_exit")
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let marker = fixture.directory.path().join("group");
    tokio::time::timeout(Duration::from_secs(10), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let group = PathBuf::from(fs::read_to_string(&marker).unwrap());
    child.kill().unwrap();
    child.wait().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while group.exists()
            && !fs::read_to_string(group.join("cgroup.events"))
                .unwrap_or_default()
                .contains("populated 0")
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("scope survived its owning harness");
}

#[tokio::test]
async fn linux_resources_child() {
    let Ok(mode) = std::env::var("BUZZ_RESOURCE_TEST_CHILD") else {
        return;
    };
    let directory = PathBuf::from(std::env::var("BUZZ_RESOURCE_TEST_DIR").unwrap());
    let worker = directory.join("worker.py");
    fs::write(&worker, r#"import sys,json,os,pathlib,subprocess
pathlib.Path(sys.argv[1]+'/started').touch()
for line in sys.stdin:
 r=json.loads(line)
 if 'id' not in r: continue
 if r['method']=='initialize':
  mask=os.umask(0);os.umask(mask)
  result={'protocolVersion':2,'pid':os.getpid(),'cgroup':pathlib.Path('/proc/self/cgroup').read_text(),'umask':mask}
 elif r['method']=='session/new': result={'sessionId':'fixture'}
 elif r['method']=='session/prompt':
  text=r['params']['prompt'][0]['text']
  if text=='hold':
   x=bytearray(120*1024*1024);x[::4096]=b'x'*(len(x)//4096)
  else:
   subprocess.Popen([sys.executable,'-c','import os,time;os.setsid();time.sleep(60)'])
   if text=='allocate':
    subprocess.Popen([sys.executable,'-c','x=bytearray(240*1024*1024);x[::4096]=b"x"*(len(x)//4096);import time;time.sleep(60)']).wait()
  result={'stopReason':'end_turn'}
 else: result={}
 print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}),flush=True)
 if r['method']=='initialize' and os.environ.get('BUZZ_RESOURCE_TEST_CHILD')=='write_race':
  subprocess.Popen([sys.executable,'-c','import time;time.sleep(0.2);x=bytearray(240*1024*1024);x[::4096]=b"x"*(len(x)//4096)'])
  import time;time.sleep(60)
"#).unwrap();
    let args = vec![
        worker.to_str().unwrap().into(),
        directory.to_str().unwrap().into(),
    ];
    if mode == "invalid" || mode == "missing_manager" {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "acp::linux_resources_tests::linux_resources_child",
                "--nocapture",
            ])
            .env("BUZZ_RESOURCE_TEST_CHILD", "refuse")
            .env("BUZZ_RESOURCE_TEST_DIR", &directory)
            .env(
                ENV,
                if mode == "invalid" {
                    "{}".to_owned()
                } else {
                    std::env::var(ENV).unwrap()
                },
            )
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                "unix:path=/nonexistent/buzz-test-bus",
            )
            .env("XDG_RUNTIME_DIR", "/nonexistent/buzz-test-runtime")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !directory.join("started").exists(),
            "failed isolation executed the worker"
        );
        return;
    }
    if mode == "refuse" {
        assert!(matches!(
            AcpClient::spawn("/usr/bin/python3", &args, &[], false).await,
            Err(super::AcpError::ResourceGuard(_))
        ));
        return;
    }
    let mut a = AcpClient::spawn("/usr/bin/python3", &args, &[], false)
        .await
        .unwrap();
    let ai = a.initialize().await.unwrap();
    let mask = Command::new("/usr/bin/python3")
        .args(["-c", "import os;print(os.umask(0))"])
        .output()
        .unwrap();
    let expected_mask: u64 = String::from_utf8(mask.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        ai["umask"].as_u64(),
        Some(expected_mask),
        "worker file creation permissions changed"
    );
    let cg = ai["cgroup"].as_str().unwrap();
    assert!(
        cg.contains("/buzzguard") && cg.contains(".slice/buzzacp-"),
        "worker is unbounded: {cg}"
    );
    let path = PathBuf::from("/sys/fs/cgroup").join(
        cg.trim()
            .split("::")
            .nth(1)
            .unwrap()
            .trim_start_matches('/'),
    );
    if mode == "write_race" {
        let error = a
            .session_prompt_with_idle_timeout(
                "fixture",
                &"x".repeat(3_000_000),
                Duration::from_secs(15),
                Duration::from_secs(20),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, super::AcpError::ResourceLimit { .. }),
            "write lost the OOM cause: {error}"
        );
        a.shutdown().await;
        return;
    }
    if mode == "parent_exit" {
        fs::write(directory.join("group"), path.to_str().unwrap()).unwrap();
        tokio::time::sleep(Duration::from_secs(60)).await;
        return;
    }
    a.session_new("/tmp", vec![], None, None).await.unwrap();
    if mode == "cleanup" {
        a.session_prompt_with_idle_timeout(
            "fixture",
            "background",
            Duration::from_secs(5),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert_eq!(a.terminate().await, super::TerminationReport::Clean);
        assert!(
            !path.exists()
                || fs::read_to_string(path.join("cgroup.events"))
                    .unwrap()
                    .contains("populated 0")
        );
        return;
    }
    let mut b = AcpClient::spawn("/usr/bin/python3", &args, &[], false)
        .await
        .unwrap();
    let bi = b.initialize().await.unwrap();
    assert_ne!(ai["cgroup"], bi["cgroup"]);
    if mode == "aggregate" {
        b.session_new("/tmp", vec![], None, None).await.unwrap();
        a.session_prompt_with_idle_timeout(
            "fixture",
            "hold",
            Duration::from_secs(15),
            Duration::from_secs(20),
        )
        .await
        .unwrap();
        let _ = b
            .session_prompt_with_idle_timeout(
                "fixture",
                "hold",
                Duration::from_secs(15),
                Duration::from_secs(20),
            )
            .await;
        let ar = a.initialize().await;
        let br = b.initialize().await;
        assert!(ar.is_ok() || br.is_ok(), "both workers died");
        assert!(
            ar.is_err() || br.is_err(),
            "aggregate budget did not contain the workers"
        );
        for error in [ar.err(), br.err()].into_iter().flatten() {
            assert!(
                matches!(error, super::AcpError::ResourceLimit { .. }),
                "{error}"
            );
        }
    } else {
        assert_eq!(
            fs::read_to_string(path.join("memory.max")).unwrap().trim(),
            "134217728"
        );
        let error = a
            .session_prompt_with_idle_timeout(
                "fixture",
                "allocate",
                Duration::from_secs(15),
                Duration::from_secs(20),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, super::AcpError::ResourceLimit { .. }),
            "wrong failure: {error}"
        );
        assert!(
            matches!(
                a.initialize().await,
                Err(super::AcpError::ResourceLimit { .. })
            ),
            "OOM was lost on a subsequent write"
        );
        assert!(b.initialize().await.is_ok(), "unrelated worker died");
    }
    a.shutdown().await;
    b.shutdown().await;
    assert!(
        !path.exists()
            || fs::read_to_string(path.join("cgroup.events"))
                .unwrap()
                .contains("populated 0")
    );
}

#[test]
fn linux_resources_policy_rejects_invalid_or_unbounded_limits() {
    for value in [
        "{}",
        "null",
        "[]",
        r#"{"slice":"../other.slice","memory_high_bytes":1,"memory_max_bytes":2}"#,
        r#"{"slice":"safe.slice","memory_high_bytes":0,"memory_max_bytes":2}"#,
        r#"{"slice":"safe.slice","memory_high_bytes":3,"memory_max_bytes":2}"#,
        r#"{"slice":"safe.slice","memory_high_bytes":1,"memory_max_bytes":18446744073709551616}"#,
    ] {
        assert!(
            super::linux_resources::Policy::parse(value).is_err(),
            "accepted {value}"
        );
    }
    assert!(super::linux_resources::Policy::parse(
        r#"{"slice":"safe.slice","memory_high_bytes":1,"memory_max_bytes":2}"#
    )
    .is_ok());
}

#[test]
fn resource_review_supervisor_exit_race() {
    let result = Command::new("/usr/bin/python3")
        .arg("-I")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/linux_resource_supervisor_test.py"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "requires a user systemd manager and cgroup v2"]
fn resource_review_rejects_ancestor_before_payload() {
    let fixture = Fixture::new(512 * 1024 * 1024);
    let command = fixture.command("refuse");
    let mut wrapped = Command::new("systemd-run");
    wrapped
        .args(["--user", "--scope", "--quiet"])
        .arg(format!(
            "--unit=buzzancestor-{}",
            uuid::Uuid::new_v4().simple()
        ))
        .arg(format!("--slice={}", fixture.slice))
        .arg("--")
        .arg(command.get_program())
        .args(command.get_args());
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            wrapped.env(key, value);
        } else {
            wrapped.env_remove(key);
        }
    }
    let result = wrapped.output().unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!fixture.directory.path().join("started").exists());
}
