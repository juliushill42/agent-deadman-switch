use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const WATERMARK: &str = "\":\"";
pub const ORIGIN: &str = "urn:titanu:marathon:agent-deadman-switch";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Healthy,
    Suspect,
    Tripped,
    Contained,
    Recovering,
    Offline,
}

impl AgentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Suspect => "suspect",
            Self::Tripped => "tripped",
            Self::Contained => "contained",
            Self::Recovering => "recovering",
            Self::Offline => "offline",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlDirective {
    Run,
    Contain,
    Stop,
}

impl ControlDirective {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Contain => "contain",
            Self::Stop => "stop",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadmanPolicy {
    pub heartbeat_timeout_secs: i64,
    pub hard_timeout_secs: i64,
    pub max_consecutive_failures: i32,
    pub fail_closed: bool,
}

impl Default for DeadmanPolicy {
    fn default() -> Self {
        Self {
            heartbeat_timeout_secs: 15,
            hard_timeout_secs: 30,
            max_consecutive_failures: 3,
            fail_closed: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHeartbeat {
    pub agent_id: Uuid,
    pub name: String,
    pub platform: String,
    pub arch: String,
    pub version: String,
    pub workload_running: bool,
    pub workload_pid: Option<u32>,
    pub consecutive_failures: i32,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentView {
    pub agent_id: Uuid,
    pub name: String,
    pub platform: String,
    pub arch: String,
    pub version: String,
    pub state: AgentState,
    pub directive: ControlDirective,
    pub armed: bool,
    pub workload_running: bool,
    pub workload_pid: Option<i64>,
    pub consecutive_failures: i32,
    pub last_seen: DateTime<Utc>,
    pub policy: DeadmanPolicy,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub state: AgentState,
    pub directive: ControlDirective,
    pub trip: bool,
    pub reason: Option<String>,
}

pub fn evaluate(
    now: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    armed: bool,
    consecutive_failures: i32,
    current_directive: ControlDirective,
    policy: &DeadmanPolicy,
) -> Evaluation {
    if !armed {
        return Evaluation {
            state: AgentState::Healthy,
            directive: ControlDirective::Run,
            trip: false,
            reason: None,
        };
    }

    if matches!(
        current_directive,
        ControlDirective::Contain | ControlDirective::Stop
    ) {
        return Evaluation {
            state: AgentState::Contained,
            directive: current_directive,
            trip: false,
            reason: None,
        };
    }

    let age = now.signed_duration_since(last_seen).num_seconds().max(0);

    if consecutive_failures >= policy.max_consecutive_failures {
        return Evaluation {
            state: AgentState::Tripped,
            directive: ControlDirective::Contain,
            trip: true,
            reason: Some(format!("consecutive_failures={consecutive_failures}")),
        };
    }

    if age >= policy.hard_timeout_secs {
        return Evaluation {
            state: AgentState::Tripped,
            directive: if policy.fail_closed {
                ControlDirective::Contain
            } else {
                ControlDirective::Run
            },
            trip: policy.fail_closed,
            reason: Some(format!("hard heartbeat timeout age={age}s")),
        };
    }

    if age >= policy.heartbeat_timeout_secs {
        return Evaluation {
            state: AgentState::Suspect,
            directive: ControlDirective::Run,
            trip: false,
            reason: Some(format!("heartbeat late age={age}s")),
        };
    }

    Evaluation {
        state: AgentState::Healthy,
        directive: ControlDirective::Run,
        trip: false,
        reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn healthy_agent_remains_running() {
        let now = Utc::now();

        let result = evaluate(
            now,
            now - Duration::seconds(2),
            true,
            0,
            ControlDirective::Run,
            &DeadmanPolicy::default(),
        );

        assert_eq!(result.state, AgentState::Healthy);
        assert_eq!(result.directive, ControlDirective::Run);
        assert!(!result.trip);
    }

    #[test]
    fn late_agent_becomes_suspect() {
        let now = Utc::now();

        let result = evaluate(
            now,
            now - Duration::seconds(20),
            true,
            0,
            ControlDirective::Run,
            &DeadmanPolicy::default(),
        );

        assert_eq!(result.state, AgentState::Suspect);
        assert!(!result.trip);
    }

    #[test]
    fn hard_timeout_trips_fail_closed_agent() {
        let now = Utc::now();

        let result = evaluate(
            now,
            now - Duration::seconds(40),
            true,
            0,
            ControlDirective::Run,
            &DeadmanPolicy::default(),
        );

        assert_eq!(result.state, AgentState::Tripped);
        assert_eq!(result.directive, ControlDirective::Contain);
        assert!(result.trip);
    }

    #[test]
    fn failure_threshold_trips() {
        let now = Utc::now();

        let result = evaluate(
            now,
            now,
            true,
            3,
            ControlDirective::Run,
            &DeadmanPolicy::default(),
        );

        assert!(result.trip);
        assert_eq!(result.directive, ControlDirective::Contain);
    }

    #[test]
    fn disarmed_agent_runs() {
        let now = Utc::now();

        let result = evaluate(
            now,
            now - Duration::hours(1),
            false,
            999,
            ControlDirective::Run,
            &DeadmanPolicy::default(),
        );

        assert_eq!(result.directive, ControlDirective::Run);
        assert!(!result.trip);
    }

    #[test]
    fn watermark_is_fixed() {
        assert_eq!(WATERMARK, "\":\"");
    }
}
