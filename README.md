# Agent Deadman Switch

Deterministic containment for supervised agents and autonomous workloads.

`":"`

## What it does

Agent Deadman Switch gives a running agent a hard control boundary.

It detects missed heartbeats, control-plane loss, repeated workload failures,
manual containment, and manual stop directives.

It can arm or disarm supervision, trip and contain workloads, recover contained
agents, fail closed locally when the control plane disappears, and preserve
state transitions in PostgreSQL and Kafka.

## Architecture

- Rust deterministic policy engine
- Rust control API
- Rust node watchdog
- TypeScript + React operational dashboard
- PostgreSQL canonical state
- Apache Kafka KRaft event stream
- Android/Termux-first
- no Docker requirement
- no root requirement
- no systemd requirement
- no paid API dependency

## Build

    ./bootstrap.sh

## Launch

    ./run.sh

Dashboard: http://127.0.0.1:4176

Control API: http://127.0.0.1:8794

## Tokens

    ./scripts/show-tokens.sh

## Supervise a real local process

Set the workload locally before starting:

    export DEADMAN_WORKLOAD_BIN="$HOME/bin/my-agent"
    export DEADMAN_WORKLOAD_ARGS="--serve --port 8000"
    ./run.sh

The remote control plane does not choose the executable. The watchdog owns only
the child process it launches.

## Inspect agents

    ./scripts/agents.sh

## Contain

    ./scripts/contain.sh AGENT_UUID "heartbeat anomaly"

## Recover

    ./scripts/recover.sh AGENT_UUID

## Default policy

- heartbeat warning: 15 seconds
- hard timeout: 30 seconds
- consecutive failures: 3
- fail closed: true

## Verify

    ./scripts/verify.sh

## Runtime smoke

After launch:

    ./scripts/smoke.sh

## Stop

    ./stop.sh

## Provenance

Owner: Julius Cameron Hill / Titan Universal AI LLC

Origin: `urn:titanu:marathon:agent-deadman-switch`

Watermark: `":"`
