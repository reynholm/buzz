//! Keep executable fixtures immutable: a concurrently forked child can retain
//! a just-written script's writer until exec and make Linux reject it (ETXTBSY).

use std::path::{Path, PathBuf};

pub(super) struct VersionFixture {
    dir: tempfile::TempDir,
}

impl VersionFixture {
    pub(super) fn new(version: &str) -> Self {
        let fixture = Self::script("codex-version.sh");
        std::fs::write(fixture.dir.path().join("version"), format!("{version}\n"))
            .expect("write version data");
        fixture
    }

    pub(super) fn script(name: &str) -> Self {
        let dir = tempfile::tempdir().expect("fixture directory");
        link_script(name, &dir.path().join("codex-acp"));
        Self { dir }
    }

    pub(super) fn binary(&self) -> PathBuf {
        self.dir.path().join("codex-acp")
    }

    pub(super) fn descendant_pid(&self) -> u32 {
        std::fs::read_to_string(self.dir.path().join("descendant.pid"))
            .expect("descendant must start and record its PID")
            .trim()
            .parse()
            .expect("descendant PID")
    }
}

impl Drop for VersionFixture {
    fn drop(&mut self) {
        // Run on assertion failures too; the descendant deliberately outlives
        // the probed shell, so dropping its TempDir alone cannot clean it up.
        let pid = std::fs::read_to_string(self.dir.path().join("descendant.pid"))
            .ok()
            .and_then(|pid| pid.trim().parse::<u32>().ok())
            .filter(|pid| *pid > 1);
        if let Some(pid) = pid {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-KILL", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

pub(super) fn link_script(name: &str, destination: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/managed_agents/discovery/tests/fixtures")
        .join(name);
    std::os::unix::fs::symlink(source, destination).expect("link immutable executable fixture");
}
