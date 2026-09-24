# Agent Deadman Switch

Deterministic containment for supervised agents and autonomous workloads.

The system gives a running local workload a hard control boundary and keeps a node-side watchdog capable of failing closed when the control plane disappears.

## Implemented architecture

- Rust deterministic policy/core crate.
- Rust control API.
- Rust node watchdog.
- TypeScript + React operational dashboard.
- PostgreSQL state.
- Apache Kafka KRaft event stream.
- Local token/shared-secret authentication.
- Android/Termux-first runtime path.
- No root requirement.
- No systemd requirement.
- No paid API dependency.

## Containment conditions

The build handles:

- missed heartbeats
- control-plane loss
- repeated workload failures
- manual containment
- manual stop directives

It can:

- arm or disarm supervision
- trip and contain workloads
- recover contained agents
- fail closed locally
- preserve state transitions through PostgreSQL and Kafka

## Bootstrap

```bash
./bootstrap.sh
```

On first launch the bootstrap creates `.env` with generated:

- admin token
- agent shared secret
- PostgreSQL password

It then:

1. Starts PostgreSQL.
2. Applies the schema.
3. Starts Kafka in KRaft mode.
4. Installs dashboard dependencies.
5. Formats Rust.
6. Runs repository verification.
7. Launches the stack unless `BUILD_ONLY=1`.

Build/verify without launching:

```bash
BUILD_ONLY=1 ./bootstrap.sh
```

## Local endpoints

Dashboard:

```text
http://127.0.0.1:4176
```

Control API:

```text
http://127.0.0.1:8794
```

## Supervise a real local process

The executable is configured locally before launch:

```bash
export DEADMAN_WORKLOAD_BIN="$HOME/bin/my-agent"
export DEADMAN_WORKLOAD_ARGS="--serve --port 8000"
./run.sh
```

The remote control plane does not select the executable. The watchdog owns only the child process it launches.

## Operations

Show generated tokens:

```bash
./scripts/show-tokens.sh
```

Inspect agents:

```bash
./scripts/agents.sh
```

Contain an agent:

```bash
./scripts/contain.sh AGENT_UUID "heartbeat anomaly"
```

Recover an agent:

```bash
./scripts/recover.sh AGENT_UUID
```

Stop:

```bash
./stop.sh
```

## Default policy

```text
heartbeat warning       15 seconds
hard timeout            30 seconds
consecutive failures     3
fail closed             true
```

## Verify

```bash
./scripts/verify.sh
```

Runtime smoke:

```bash
./scripts/smoke.sh
```

## Workspace

Rust members:

```text
crates/deadman-core
apps/deadman-control
apps/deadman-agent
```

Supporting runtime:

```text
web/
infra/
migrations/
scripts/
```

## Provenance

Owner: Julius Cameron Hill / Titan Universal AI LLC  
Origin: `urn:titanu:marathon:agent-deadman-switch`  
Watermark: `":"`
