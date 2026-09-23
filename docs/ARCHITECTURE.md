# Agent Deadman Switch

Agent Deadman Switch supervises a locally configured workload and applies
deterministic containment rules when the control contract is violated.

## Components

### deadman-core

Pure deterministic policy evaluation.

Inputs:

- armed state
- current directive
- heartbeat age
- consecutive workload failures
- heartbeat warning threshold
- hard heartbeat threshold
- failure threshold
- fail-open / fail-closed policy

Outputs:

- agent state
- execution directive
- trip decision
- reason

### deadman-control

The control plane stores agent state in PostgreSQL and continuously evaluates
registered agents.

The evaluator runs independently of incoming heartbeat requests.

### deadman-agent

Runs on the supervised node.

It can supervise a program configured locally through:

`DEADMAN_WORKLOAD_BIN`

and:

`DEADMAN_WORKLOAD_ARGS`

The control plane cannot upload or replace that command.

The watchdog can only:

- allow the locally configured workload to run
- contain that workload
- stop the watchdog

There is no arbitrary remote shell endpoint.

## States

### healthy

Heartbeat and workload state are inside policy limits.

### suspect

Heartbeat has exceeded the warning threshold but not the hard threshold.

### tripped

A deterministic policy boundary has been crossed.

### contained

The workload is not permitted to run.

### recovering

The operator changed the directive back to run and the workload is returning
to service.

## Directives

### run

The local watchdog may run its configured workload.

### contain

The watchdog terminates its own supervised child process and remains alive.

### stop

The watchdog terminates its own supervised child and exits.

## Fail closed

When fail-closed is enabled, the local watchdog also tracks the elapsed time
since it last contacted the control plane.

If control-plane connectivity exceeds the hard timeout, the watchdog contains
its supervised child locally.

That enforcement does not depend on the PostgreSQL evaluator being reachable.

## Storage

PostgreSQL stores canonical control state.

Kafka KRaft receives immutable lifecycle events.

## Runtime

Android/Termux-first.

No Docker requirement.

No root requirement.

No systemd requirement.

Watermark: `":"`
