use std::{
    env, fs,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use deadman_core::{AgentHeartbeat, ControlDirective, DeadmanPolicy, ORIGIN, WATERMARK};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use sysinfo::System;
use tokio::{
    process::{Child, Command},
    time,
};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct DirectiveResponse {
    directive: ControlDirective,
    armed: bool,
    policy: DeadmanPolicy,
}

struct Supervisor {
    child: Option<Child>,
    workload_bin: Option<String>,
    workload_args: Vec<String>,
    consecutive_failures: i32,
}

impl Supervisor {
    fn new() -> Self {
        let workload_bin = env::var("DEADMAN_WORKLOAD_BIN")
            .ok()
            .filter(|value| !value.trim().is_empty());

        let workload_args = env::var("DEADMAN_WORKLOAD_ARGS")
            .ok()
            .map(|value| value.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();

        Self {
            child: None,
            workload_bin,
            workload_args,
            consecutive_failures: 0,
        }
    }

    async fn is_running(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return false;
        };

        match child.try_wait() {
            Ok(None) => true,

            Ok(Some(status)) => {
                warn!("workload exited status={status}");

                self.child = None;
                self.consecutive_failures += 1;
                false
            }

            Err(err) => {
                error!("workload status failed: {err}");

                self.child = None;
                self.consecutive_failures += 1;
                false
            }
        }
    }

    async fn ensure_running(&mut self) -> Result<()> {
        if self.is_running().await {
            return Ok(());
        }

        let Some(bin) = self.workload_bin.as_ref() else {
            return Ok(());
        };

        info!(
            workload = %bin,
            "starting supervised workload"
        );

        let mut command = Command::new(bin);

        command
            .args(&self.workload_args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);

        let child = command
            .spawn()
            .with_context(|| format!("failed to start workload {bin}"))?;

        self.child = Some(child);

        Ok(())
    }

    async fn contain(&mut self, reason: &str) -> Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };

        warn!(reason, "containing supervised workload");

        child.kill().await?;
        let _ = child.wait().await;

        Ok(())
    }

    fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(Child::id)
    }
}

fn state_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME is not set")?;

    let path = PathBuf::from(home)
        .join(".config")
        .join("titanu-agent-deadman");

    fs::create_dir_all(&path)?;

    Ok(path)
}

fn load_agent_id() -> Result<Uuid> {
    if let Ok(value) = env::var("DEADMAN_AGENT_ID") {
        return Ok(Uuid::parse_str(&value)?);
    }

    let path = state_dir()?.join("agent-id");

    if path.exists() {
        let value = fs::read_to_string(&path)?;

        return Ok(Uuid::parse_str(value.trim())?);
    }

    let id = Uuid::new_v4();

    fs::write(path, id.to_string())?;

    Ok(id)
}

fn agent_name() -> String {
    env::var("DEADMAN_AGENT_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env::var("HOSTNAME").ok())
        .unwrap_or_else(|| format!("agent-{}", std::env::consts::ARCH))
}

async fn heartbeat(
    client: &Client,
    base: &str,
    secret: &str,
    agent_id: Uuid,
    supervisor: &mut Supervisor,
) -> Result<DirectiveResponse> {
    let running = supervisor.is_running().await;

    let mut system = System::new_all();

    system.refresh_all();

    let payload = AgentHeartbeat {
        agent_id,
        name: agent_name(),
        platform: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        workload_running: running,
        workload_pid: supervisor.pid(),
        consecutive_failures: supervisor.consecutive_failures,
        metadata: json!({
            "cpu_count":
                system.cpus().len(),
            "total_memory":
                system.total_memory(),
            "used_memory":
                system.used_memory(),
            "origin":
                ORIGIN,
            "watermark":
                WATERMARK
        }),
    };

    let response = client
        .post(format!("{base}/api/v1/agent/heartbeat"))
        .bearer_auth(secret)
        .json(&payload)
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json().await?)
}

async fn fetch_directive(
    client: &Client,
    base: &str,
    secret: &str,
    agent_id: Uuid,
) -> Result<DirectiveResponse> {
    Ok(client
        .get(format!("{base}/api/v1/agent/{agent_id}/directive"))
        .bearer_auth(secret)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "deadman_agent=info".into()),
        )
        .init();

    let agent_id = load_agent_id()?;

    let base = env::var("DEADMAN_CONTROL_URL").unwrap_or_else(|_| "http://127.0.0.1:8794".into());

    let secret =
        env::var("DEADMAN_AGENT_SHARED_SECRET").context("DEADMAN_AGENT_SHARED_SECRET required")?;

    let client = Client::builder().timeout(Duration::from_secs(8)).build()?;

    let mut supervisor = Supervisor::new();

    let mut policy = DeadmanPolicy::default();

    let mut directive = ControlDirective::Run;

    let mut armed = true;

    let mut last_control_contact = Instant::now();

    info!(
        %agent_id,
        %base,
        "deadman agent started"
    );

    loop {
        if armed && directive == ControlDirective::Run {
            if let Err(err) = supervisor.ensure_running().await {
                error!("workload launch failed: {err}");

                supervisor.consecutive_failures += 1;
            }
        }

        match heartbeat(&client, &base, &secret, agent_id, &mut supervisor).await {
            Ok(control) => {
                last_control_contact = Instant::now();

                policy = control.policy;

                directive = control.directive;

                armed = control.armed;
            }

            Err(err) => {
                warn!("heartbeat failed: {err}");

                match fetch_directive(&client, &base, &secret, agent_id).await {
                    Ok(control) => {
                        last_control_contact = Instant::now();

                        policy = control.policy;

                        directive = control.directive;

                        armed = control.armed;
                    }

                    Err(err) => {
                        warn!("control poll failed: {err}");
                    }
                }
            }
        }

        match directive {
            ControlDirective::Run => {}

            ControlDirective::Contain => {
                supervisor
                    .contain("control-plane containment directive")
                    .await?;
            }

            ControlDirective::Stop => {
                supervisor.contain("control-plane stop directive").await?;

                info!("stop directive received");

                return Ok(());
            }
        }

        if armed
            && policy.fail_closed
            && last_control_contact.elapsed().as_secs() >= policy.hard_timeout_secs.max(1) as u64
        {
            supervisor
                .contain("local fail-closed control timeout")
                .await?;
        }

        time::sleep(Duration::from_secs(3)).await;
    }
}
