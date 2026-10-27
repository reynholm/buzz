//! Bounded native discovery with ownership of the complete CLI process group.
use anyhow::{bail, Context, Result};
use nix::{
    errno::Errno,
    sys::signal::{killpg, Signal},
    unistd::Pid,
};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::Instant};

struct DiscoveryGroup(Option<Pid>);

impl DiscoveryGroup {
    fn kill(&self) -> Result<()> {
        if let Some(group) = self.0 {
            match killpg(group, Signal::SIGKILL) {
                Ok(()) | Err(Errno::ESRCH) => {}
                Err(error) => return Err(error).context("killing native discovery group"),
            }
        }
        Ok(())
    }

    async fn retire(&mut self, child: &mut tokio::process::Child) -> Result<()> {
        self.kill()?;
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            let reaped = child.try_wait()?.is_some();
            if let Some(group) = self.0 {
                match killpg(group, None) {
                    Err(Errno::ESRCH) if reaped => {
                        self.0 = None;
                        return Ok(());
                    }
                    Ok(()) | Err(Errno::ESRCH) => {}
                    Err(error) => return Err(error).context("verifying native discovery cleanup"),
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        bail!("native discovery process group did not retire within the cleanup deadline")
    }
}

impl Drop for DiscoveryGroup {
    fn drop(&mut self) {
        // Also runs if the caller cancels startup while discovery is awaiting I/O.
        if let Err(error) = self.kill() {
            tracing::warn!(%error, "native discovery group cleanup failed");
        }
    }
}

pub(super) async fn output(mut command: Command) -> Result<String> {
    const LIMIT: u64 = 1024 * 1024;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .context("starting native discovery")?;
    let pid = child
        .id()
        .context("native discovery process identity missing")?;
    let mut group = DiscoveryGroup(Some(Pid::from_raw(pid as i32)));
    let capture = async {
        let pipe = child
            .stdout
            .take()
            .context("native discovery stdout missing")?;
        let mut bytes = Vec::new();
        pipe.take(LIMIT + 1).read_to_end(&mut bytes).await?;
        if bytes.len() as u64 > LIMIT {
            bail!("native discovery output exceeded the limit");
        }
        if !child.wait().await?.success() {
            bail!("native discovery exited unsuccessfully");
        }
        String::from_utf8(bytes).context("native discovery output is not UTF-8")
    };
    let outcome = match tokio::time::timeout(Duration::from_secs(3), capture).await {
        Ok(result) => result,
        Err(error) => Err(error).context("native discovery timed out"),
    };
    // Cleanup is required even after clean output: a CLI can exit while a
    // descendant closes stdout and keeps running. Failure rejects discovery.
    group.retire(&mut child).await?;
    outcome
}
