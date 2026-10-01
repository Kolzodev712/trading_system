use anyhow::{bail, Context, Result};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub fn allocate_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("tests/workflow_harness")
        .to_path_buf()
}

pub fn k_paper_dir() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("K_PAPER_DIR") {
        let path = PathBuf::from(p);
        if path.join("Cargo.toml").exists() {
            return Ok(path);
        }
        bail!(
            "K_PAPER_DIR does not look like a k-paper checkout: {}",
            path.display()
        );
    }
    let sibling = workspace_root().parent().unwrap().join("k-paper");
    if sibling.join("Cargo.toml").exists() {
        return Ok(sibling);
    }
    bail!(
        "k-paper checkout not found at {} (set K_PAPER_DIR). Missing k-paper is a failure, not a skip.",
        sibling.display()
    );
}

pub fn ui_dir() -> PathBuf {
    workspace_root().parent().unwrap().join("trading-system-ui")
}

pub fn require_mongo() -> Result<()> {
    match std::net::TcpStream::connect_timeout(
        &"127.0.0.1:27017".parse().unwrap(),
        Duration::from_secs(1),
    ) {
        Ok(_) => Ok(()),
        Err(e) => bail!("MongoDB is required on 127.0.0.1:27017: {e}"),
    }
}

pub fn debug_bin(workspace_or_crate: &Path, name: &str) -> PathBuf {
    let dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_or_crate.join("target"));
    dir.join("debug").join(name)
}

pub fn cargo_build(dir: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("cargo")
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("cargo in {}", dir.display()))?;
    if !status.success() {
        bail!("cargo {:?} failed in {}", args, dir.display());
    }
    Ok(())
}

pub struct ChildProc {
    pub name: String,
    child: Child,
    keep_alive: bool,
}

impl ChildProc {
    pub fn spawn(name: &str, mut cmd: Command, log_path: &Path) -> Result<Self> {
        let log = std::fs::File::create(log_path)
            .with_context(|| format!("log {}", log_path.display()))?;
        let err = log.try_clone()?;
        cmd.stdout(Stdio::from(log));
        cmd.stderr(Stdio::from(err));
        let child = cmd.spawn().with_context(|| format!("spawn {name}"))?;
        Ok(Self {
            name: name.into(),
            child,
            keep_alive: false,
        })
    }

    pub fn disarm(&mut self) {
        self.keep_alive = true;
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn terminate(&mut self) {
        let pid = self.child.id();
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = pid;
    }

    pub fn try_wait(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
}

impl Drop for ChildProc {
    fn drop(&mut self) {
        if !self.keep_alive {
            self.terminate();
        }
    }
}

pub async fn wait_http(url: &str, timeout: Duration) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let deadline = Instant::now() + timeout;
    let mut last = String::new();
    while Instant::now() < deadline {
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => last = format!("status {}", resp.status()),
            Err(e) => last = e.to_string(),
        }
        sleep(Duration::from_millis(200)).await;
    }
    bail!("timeout waiting for {url}: {last}")
}

pub async fn wait_health_cex_ready(url: &str, timeout: Duration) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let deadline = Instant::now() + timeout;
    let mut last = String::new();
    while Instant::now() < deadline {
        match client.get(url).send().await {
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                last = body.clone();
                if body.contains("cex_ready=true") {
                    return Ok(());
                }
            }
            Err(e) => last = e.to_string(),
        }
        sleep(Duration::from_millis(250)).await;
    }
    bail!("timeout waiting for cex_ready at {url}: {last}")
}
