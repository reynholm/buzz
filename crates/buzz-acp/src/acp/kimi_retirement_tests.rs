//! Real process groups, with a synthetic ACP peer; no installed agent is touched.
use super::*;
use std::{path::PathBuf, time::Duration};

const PEER: &str = r#"
import json,os,subprocess,sys
for line in sys.stdin:
    req=json.loads(line)
    if 'id' not in req: continue
    method=req.get('method'); result={}
    if method=='initialize': result={'protocolVersion':1,'agentInfo':{'name':'Kimi Code CLI','version':'synthetic-retirement'}}
    elif method=='session/new': result={'sessionId':'owned-session'}
    elif method=='session/prompt':
        program='import os,sys,time\nwhile not os.path.exists(sys.argv[1]):\n if os.path.exists(sys.argv[1]+".exec"): os.execv(sys.executable,[sys.executable,"-c","import os,sys,time\\nwhile not os.path.exists(sys.argv[1]): time.sleep(.01)",sys.argv[1]])\n time.sleep(.01)'
        child=subprocess.Popen([sys.executable,'-c',program,sys.argv[1]],preexec_fn=lambda:os.setpgid(0,0),stdout=subprocess.DEVNULL)
        with open(sys.argv[2],'w') as out: out.write(str(child.pid))
        result={'stopReason':'end_turn'}
    print(json.dumps({'jsonrpc':'2.0','id':req['id'],'result':result}),flush=True)
"#;

struct Finish(PathBuf);
impl Drop for Finish {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, "finish");
    }
}

#[tokio::test]
async fn kimi_observed_separate_group_is_never_lost_after_the_adapter_dies() {
    exercise(false, false).await;
}

#[tokio::test]
async fn kimi_missing_observed_descendant_proof_blocks_a_clean_retirement() {
    exercise(true, false).await;
}

#[tokio::test]
async fn kimi_exec_after_adapter_death_does_not_masquerade_as_child_exit() {
    exercise(false, true).await;
}

async fn exercise(discard_observation_proof: bool, exec_after_death: bool) {
    let dir = tempfile::tempdir().unwrap();
    let finish = Finish(dir.path().join("finish"));
    let child_path = dir.path().join("child");
    let mut client = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            PEER.into(),
            finish.0.to_string_lossy().into(),
            child_path.to_string_lossy().into(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    client.initialize().await.unwrap();
    client
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    let before = client.termination.evidence.clone();
    client
        .session_prompt_with_idle_timeout(
            "owned-session",
            "start one background child",
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    let child: u32 = std::fs::read_to_string(child_path)
        .unwrap()
        .parse()
        .unwrap();
    assert!(client.background_child_pids().unwrap().contains(&child));
    assert!(client.observed_background_instances.contains_key(&child));
    let original = client.observed_background_instances[&child].clone();
    if discard_observation_proof {
        client.termination.evidence = before;
    }
    client.child.start_kill().unwrap();
    client.child.wait().await.unwrap();
    if exec_after_death {
        std::fs::write(finish.0.with_extension("exec"), "exec").unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let tree = crate::process_tree::snapshot().unwrap();
                if tree
                    .process_instance(child)
                    .is_some_and(|current| current != &original)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    } else {
        assert!(
            client.background_child_pids().unwrap().contains(&child),
            "known live work cannot become a false completion after leader reaping"
        );
    }
    let report = client.terminate().await;
    let alive = crate::process_tree::snapshot()
        .unwrap()
        .process_instance(child)
        .is_some();
    assert!(
        !alive || report != TerminationReport::Clean,
        "a previously observed group cannot remain live behind a Clean retirement"
    );
    if discard_observation_proof || exec_after_death {
        assert_eq!(report, TerminationReport::Unverified);
    }
    drop(finish);
    client.shutdown().await;
}
