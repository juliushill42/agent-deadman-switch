use std::{env, net::SocketAddr, time::Duration};

use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use deadman_core::{
    evaluate, AgentHeartbeat, AgentState, AgentView, ControlDirective, DeadmanPolicy, ORIGIN,
    WATERMARK,
};
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use tokio::time;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use uuid::Uuid;

const MIGRATION: &str = include_str!("../../../migrations/0001_init.sql");

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    producer: FutureProducer,
    kafka_topic: String,
    admin_token: String,
    agent_secret: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized")
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    fn internal(err: impl std::fmt::Display) -> Self {
        error!("{err}");

        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.message,
                "watermark": WATERMARK
            })),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct PolicyInput {
    heartbeat_timeout_secs: i64,
    hard_timeout_secs: i64,
    max_consecutive_failures: i32,
    fail_closed: bool,
}

#[derive(Debug, Deserialize)]
struct ManualDirective {
    directive: String,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArmInput {
    armed: bool,
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
struct EventView {
    event_id: i64,
    event_type: String,
    agent_id: Option<Uuid>,
    payload: Value,
    occurred_at: DateTime<Utc>,
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

fn require_admin(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    match bearer(headers) {
        Some(token) if token == state.admin_token => Ok(()),
        _ => Err(ApiError::unauthorized()),
    }
}

fn require_agent(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    match bearer(headers) {
        Some(token) if token == state.agent_secret => Ok(()),
        _ => Err(ApiError::unauthorized()),
    }
}

fn parse_state(value: &str) -> AgentState {
    match value {
        "suspect" => AgentState::Suspect,
        "tripped" => AgentState::Tripped,
        "contained" => AgentState::Contained,
        "recovering" => AgentState::Recovering,
        "offline" => AgentState::Offline,
        _ => AgentState::Healthy,
    }
}

fn parse_directive(value: &str) -> ControlDirective {
    match value {
        "contain" => ControlDirective::Contain,
        "stop" => ControlDirective::Stop,
        _ => ControlDirective::Run,
    }
}

fn policy_from_row(
    heartbeat_timeout_secs: i64,
    hard_timeout_secs: i64,
    max_consecutive_failures: i32,
    fail_closed: bool,
) -> DeadmanPolicy {
    DeadmanPolicy {
        heartbeat_timeout_secs,
        hard_timeout_secs,
        max_consecutive_failures,
        fail_closed,
    }
}

fn agent_from_row(row: &sqlx::postgres::PgRow) -> AgentView {
    AgentView {
        agent_id: row.get("agent_id"),
        name: row.get("name"),
        platform: row.get("platform"),
        arch: row.get("arch"),
        version: row.get("version"),
        state: parse_state(row.get::<String, _>("state").as_str()),
        directive: parse_directive(row.get::<String, _>("directive").as_str()),
        armed: row.get("armed"),
        workload_running: row.get("workload_running"),
        workload_pid: row.get("workload_pid"),
        consecutive_failures: row.get("consecutive_failures"),
        last_seen: row.get("last_seen"),
        policy: policy_from_row(
            row.get("heartbeat_timeout_secs"),
            row.get("hard_timeout_secs"),
            row.get("max_consecutive_failures"),
            row.get("fail_closed"),
        ),
        metadata: row.get("metadata"),
    }
}

async fn emit_event(
    state: &AppState,
    event_type: &str,
    agent_id: Option<Uuid>,
    payload: Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO events
          (
            event_type,
            agent_id,
            payload
          )
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(event_type)
    .bind(agent_id)
    .bind(&payload)
    .execute(&state.pool)
    .await?;

    let event = json!({
        "event_type": event_type,
        "agent_id": agent_id,
        "payload": payload,
        "origin": ORIGIN,
        "watermark": WATERMARK,
        "occurred_at": Utc::now()
    });

    let serialized = serde_json::to_string(&event)?;

    let key = agent_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "deadman-control".into());

    let record = FutureRecord::to(&state.kafka_topic)
        .key(&key)
        .payload(&serialized);

    if let Err((err, _)) = state
        .producer
        .send(
            record,
            rdkafka::util::Timeout::After(Duration::from_secs(2)),
        )
        .await
    {
        error!("Kafka publish failed: {err}");
    }

    Ok(())
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let database = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();

    Json(json!({
        "status": if database {
            "ok"
        } else {
            "degraded"
        },
        "database": database,
        "origin": ORIGIN,
        "watermark": WATERMARK
    }))
}

async fn heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(heartbeat): Json<AgentHeartbeat>,
) -> Result<Json<Value>, ApiError> {
    require_agent(&headers, &state)?;

    if heartbeat.name.trim().is_empty() {
        return Err(ApiError::bad("agent name cannot be empty"));
    }

    sqlx::query(
        r#"
        INSERT INTO agents
          (
            agent_id,
            name,
            platform,
            arch,
            version,
            state,
            directive,
            armed,
            workload_running,
            workload_pid,
            consecutive_failures,
            metadata,
            last_seen
          )
        VALUES
          (
            $1,
            $2,
            $3,
            $4,
            $5,
            'healthy',
            'run',
            TRUE,
            $6,
            $7,
            $8,
            $9,
            NOW()
          )
        ON CONFLICT (agent_id)
        DO UPDATE SET
          name = EXCLUDED.name,
          platform = EXCLUDED.platform,
          arch = EXCLUDED.arch,
          version = EXCLUDED.version,
          workload_running = EXCLUDED.workload_running,
          workload_pid = EXCLUDED.workload_pid,
          consecutive_failures =
            EXCLUDED.consecutive_failures,
          metadata = EXCLUDED.metadata,
          last_seen = NOW(),
          updated_at = NOW()
        "#,
    )
    .bind(heartbeat.agent_id)
    .bind(heartbeat.name.trim())
    .bind(&heartbeat.platform)
    .bind(&heartbeat.arch)
    .bind(&heartbeat.version)
    .bind(heartbeat.workload_running)
    .bind(heartbeat.workload_pid.map(i64::from))
    .bind(heartbeat.consecutive_failures)
    .bind(&heartbeat.metadata)
    .execute(&state.pool)
    .await
    .map_err(ApiError::internal)?;

    let row = sqlx::query(
        r#"
        SELECT
          agent_id,
          name,
          platform,
          arch,
          version,
          state,
          directive,
          armed,
          workload_running,
          workload_pid,
          consecutive_failures,
          last_seen,
          heartbeat_timeout_secs,
          hard_timeout_secs,
          max_consecutive_failures,
          fail_closed,
          metadata
        FROM agents
        WHERE agent_id = $1
        "#,
    )
    .bind(heartbeat.agent_id)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::internal)?;

    let view = agent_from_row(&row);

    let evaluation = evaluate(
        Utc::now(),
        view.last_seen,
        view.armed,
        view.consecutive_failures,
        view.directive,
        &view.policy,
    );

    if evaluation.state != view.state {
        sqlx::query(
            r#"
            UPDATE agents
            SET
              state = $1,
              directive = $2,
              updated_at = NOW()
            WHERE agent_id = $3
            "#,
        )
        .bind(evaluation.state.as_str())
        .bind(evaluation.directive.as_str())
        .bind(heartbeat.agent_id)
        .execute(&state.pool)
        .await
        .map_err(ApiError::internal)?;
    }

    if evaluation.trip {
        emit_event(
            &state,
            "agent.tripped",
            Some(heartbeat.agent_id),
            json!({
                "reason":
                    evaluation.reason,
                "directive":
                    evaluation.directive.as_str()
            }),
        )
        .await
        .map_err(ApiError::internal)?;
    }

    Ok(Json(json!({
        "accepted": true,
        "agent_id": heartbeat.agent_id,
        "state": evaluation.state,
        "directive": evaluation.directive,
        "armed": view.armed,
        "policy": view.policy
    })))
}

async fn list_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AgentView>>, ApiError> {
    require_admin(&headers, &state)?;

    let rows = sqlx::query(
        r#"
        SELECT
          agent_id,
          name,
          platform,
          arch,
          version,
          state,
          directive,
          armed,
          workload_running,
          workload_pid,
          consecutive_failures,
          last_seen,
          heartbeat_timeout_secs,
          hard_timeout_secs,
          max_consecutive_failures,
          fail_closed,
          metadata
        FROM agents
        ORDER BY
          last_seen DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(rows.iter().map(agent_from_row).collect()))
}

async fn get_agent_directive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    require_agent(&headers, &state)?;

    let row = sqlx::query(
        r#"
        SELECT
          directive,
          armed,
          heartbeat_timeout_secs,
          hard_timeout_secs,
          max_consecutive_failures,
          fail_closed
        FROM agents
        WHERE agent_id = $1
        "#,
    )
    .bind(agent_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(|| ApiError::not_found("agent not found"))?;

    Ok(Json(json!({
        "directive":
            row.get::<String, _>("directive"),
        "armed":
            row.get::<bool, _>("armed"),
        "policy": {
            "heartbeat_timeout_secs":
                row.get::<i64, _>(
                    "heartbeat_timeout_secs"
                ),
            "hard_timeout_secs":
                row.get::<i64, _>(
                    "hard_timeout_secs"
                ),
            "max_consecutive_failures":
                row.get::<i32, _>(
                    "max_consecutive_failures"
                ),
            "fail_closed":
                row.get::<bool, _>(
                    "fail_closed"
                )
        }
    })))
}

async fn set_arm(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent_id): Path<Uuid>,
    Json(input): Json<ArmInput>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&headers, &state)?;

    let changed = sqlx::query(
        r#"
            UPDATE agents
            SET
              armed = $1,
              state =
                CASE
                  WHEN $1
                    THEN state
                  ELSE 'healthy'
                END,
              directive =
                CASE
                  WHEN $1
                    THEN directive
                  ELSE 'run'
                END,
              updated_at = NOW()
            WHERE agent_id = $2
            "#,
    )
    .bind(input.armed)
    .bind(agent_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::internal)?
    .rows_affected();

    if changed == 0 {
        return Err(ApiError::not_found("agent not found"));
    }

    emit_event(
        &state,
        if input.armed {
            "agent.armed"
        } else {
            "agent.disarmed"
        },
        Some(agent_id),
        json!({
            "armed": input.armed
        }),
    )
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "armed": input.armed
    })))
}

async fn set_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent_id): Path<Uuid>,
    Json(input): Json<PolicyInput>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&headers, &state)?;

    if input.heartbeat_timeout_secs < 2 {
        return Err(ApiError::bad("heartbeat_timeout_secs must be >= 2"));
    }

    if input.hard_timeout_secs <= input.heartbeat_timeout_secs {
        return Err(ApiError::bad(
            "hard_timeout_secs must exceed heartbeat_timeout_secs",
        ));
    }

    if input.max_consecutive_failures < 1 {
        return Err(ApiError::bad("max_consecutive_failures must be >= 1"));
    }

    let changed = sqlx::query(
        r#"
            UPDATE agents
            SET
              heartbeat_timeout_secs = $1,
              hard_timeout_secs = $2,
              max_consecutive_failures = $3,
              fail_closed = $4,
              updated_at = NOW()
            WHERE agent_id = $5
            "#,
    )
    .bind(input.heartbeat_timeout_secs)
    .bind(input.hard_timeout_secs)
    .bind(input.max_consecutive_failures)
    .bind(input.fail_closed)
    .bind(agent_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::internal)?
    .rows_affected();

    if changed == 0 {
        return Err(ApiError::not_found("agent not found"));
    }

    emit_event(
        &state,
        "policy.updated",
        Some(agent_id),
        json!({
            "heartbeat_timeout_secs":
                input.heartbeat_timeout_secs,
            "hard_timeout_secs":
                input.hard_timeout_secs,
            "max_consecutive_failures":
                input.max_consecutive_failures,
            "fail_closed":
                input.fail_closed
        }),
    )
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(json!({
        "updated": true
    })))
}

async fn set_directive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent_id): Path<Uuid>,
    Json(input): Json<ManualDirective>,
) -> Result<Json<Value>, ApiError> {
    require_admin(&headers, &state)?;

    let directive = match input.directive.as_str() {
        "run" => ControlDirective::Run,
        "contain" => ControlDirective::Contain,
        "stop" => ControlDirective::Stop,
        _ => {
            return Err(ApiError::bad("directive must be run, contain, or stop"));
        }
    };

    let state_value = match directive {
        ControlDirective::Run => "recovering",

        ControlDirective::Contain | ControlDirective::Stop => "contained",
    };

    let changed = sqlx::query(
        r#"
            UPDATE agents
            SET
              directive = $1,
              state = $2,
              updated_at = NOW()
            WHERE agent_id = $3
            "#,
    )
    .bind(directive.as_str())
    .bind(state_value)
    .bind(agent_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::internal)?
    .rows_affected();

    if changed == 0 {
        return Err(ApiError::not_found("agent not found"));
    }

    emit_event(
        &state,
        "directive.changed",
        Some(agent_id),
        json!({
            "directive":
                directive.as_str(),
            "reason":
                input.reason
        }),
    )
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(json!({
        "agent_id":
            agent_id,
        "directive":
            directive
    })))
}

async fn list_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<EventView>>, ApiError> {
    require_admin(&headers, &state)?;

    let limit = query.limit.unwrap_or(100).clamp(1, 500);

    let rows = sqlx::query(
        r#"
            SELECT
              event_id,
              event_type,
              agent_id,
              payload,
              occurred_at
            FROM events
            ORDER BY
              event_id DESC
            LIMIT $1
            "#,
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(
        rows.into_iter()
            .map(|row| EventView {
                event_id: row.get("event_id"),
                event_type: row.get("event_type"),
                agent_id: row.get("agent_id"),
                payload: row.get("payload"),
                occurred_at: row.get("occurred_at"),
            })
            .collect(),
    ))
}

async fn evaluator_loop(state: AppState) {
    let mut interval = time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        if let Err(err) = evaluate_all_agents(&state).await {
            error!("deadman evaluation failed: {err}");
        }
    }
}

async fn evaluate_all_agents(state: &AppState) -> Result<()> {
    let rows = sqlx::query(
        r#"
            SELECT
              agent_id,
              state,
              directive,
              armed,
              consecutive_failures,
              last_seen,
              heartbeat_timeout_secs,
              hard_timeout_secs,
              max_consecutive_failures,
              fail_closed
            FROM agents
            "#,
    )
    .fetch_all(&state.pool)
    .await?;

    let now = Utc::now();

    for row in rows {
        let agent_id: Uuid = row.get("agent_id");

        let old_state = parse_state(row.get::<String, _>("state").as_str());

        let old_directive = parse_directive(row.get::<String, _>("directive").as_str());

        let policy = policy_from_row(
            row.get("heartbeat_timeout_secs"),
            row.get("hard_timeout_secs"),
            row.get("max_consecutive_failures"),
            row.get("fail_closed"),
        );

        let evaluation = evaluate(
            now,
            row.get("last_seen"),
            row.get("armed"),
            row.get("consecutive_failures"),
            old_directive,
            &policy,
        );

        if evaluation.state != old_state || evaluation.directive != old_directive {
            sqlx::query(
                r#"
                UPDATE agents
                SET
                  state = $1,
                  directive = $2,
                  updated_at = NOW()
                WHERE agent_id = $3
                "#,
            )
            .bind(evaluation.state.as_str())
            .bind(evaluation.directive.as_str())
            .bind(agent_id)
            .execute(&state.pool)
            .await?;

            emit_event(
                state,
                if evaluation.trip {
                    "agent.tripped"
                } else {
                    "agent.state_changed"
                },
                Some(agent_id),
                json!({
                    "previous_state":
                        old_state.as_str(),
                    "state":
                        evaluation.state.as_str(),
                    "previous_directive":
                        old_directive.as_str(),
                    "directive":
                        evaluation.directive.as_str(),
                    "reason":
                        evaluation.reason
                }),
            )
            .await?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "deadman_control=info,tower_http=info".into()),
        )
        .init();

    let database_url = env::var("DATABASE_URL").context("DATABASE_URL required")?;

    let admin_token = env::var("DEADMAN_ADMIN_TOKEN").context("DEADMAN_ADMIN_TOKEN required")?;

    let agent_secret =
        env::var("DEADMAN_AGENT_SHARED_SECRET").context("DEADMAN_AGENT_SHARED_SECRET required")?;

    if admin_token.len() < 32 || agent_secret.len() < 32 {
        anyhow::bail!("deadman secrets must be at least 32 characters");
    }

    let kafka_brokers = env::var("KAFKA_BROKERS").unwrap_or_else(|_| "127.0.0.1:29112".into());

    let kafka_topic = env::var("KAFKA_TOPIC").unwrap_or_else(|_| "agent-deadman-events".into());

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await
        .context("PostgreSQL connection failed")?;

    sqlx::raw_sql(MIGRATION)
        .execute(&pool)
        .await
        .context("database migration failed")?;

    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &kafka_brokers)
        .set("message.timeout.ms", "3000")
        .set("acks", "all")
        .create()
        .context("Kafka producer creation failed")?;

    let state = AppState {
        pool,
        producer,
        kafka_topic,
        admin_token,
        agent_secret,
    };

    tokio::spawn(evaluator_loop(state.clone()));

    let app = Router::new()
        .route("/healthz", get(health))
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/agents/:agent_id/arm", post(set_arm))
        .route("/api/v1/agents/:agent_id/policy", post(set_policy))
        .route("/api/v1/agents/:agent_id/directive", post(set_directive))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/agent/heartbeat", post(heartbeat))
        .route(
            "/api/v1/agent/:agent_id/directive",
            get(get_agent_directive),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = env::var("DEADMAN_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8794".into())
        .parse()
        .context("invalid DEADMAN_BIND")?;

    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("Agent Deadman control plane listening on http://{addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
