CREATE TABLE IF NOT EXISTS agents (
    agent_id UUID PRIMARY KEY,

    name TEXT NOT NULL,
    platform TEXT NOT NULL,
    arch TEXT NOT NULL,
    version TEXT NOT NULL,

    state TEXT NOT NULL DEFAULT 'healthy'
        CHECK (
            state IN (
                'healthy',
                'suspect',
                'tripped',
                'contained',
                'recovering',
                'offline'
            )
        ),

    directive TEXT NOT NULL DEFAULT 'run'
        CHECK (
            directive IN (
                'run',
                'contain',
                'stop'
            )
        ),

    armed BOOLEAN NOT NULL DEFAULT TRUE,

    workload_running BOOLEAN
        NOT NULL
        DEFAULT FALSE,

    workload_pid BIGINT,

    consecutive_failures INTEGER
        NOT NULL
        DEFAULT 0,

    heartbeat_timeout_secs BIGINT
        NOT NULL
        DEFAULT 15,

    hard_timeout_secs BIGINT
        NOT NULL
        DEFAULT 30,

    max_consecutive_failures INTEGER
        NOT NULL
        DEFAULT 3,

    fail_closed BOOLEAN
        NOT NULL
        DEFAULT TRUE,

    metadata JSONB
        NOT NULL
        DEFAULT '{}'::jsonb,

    last_seen TIMESTAMPTZ
        NOT NULL
        DEFAULT NOW(),

    created_at TIMESTAMPTZ
        NOT NULL
        DEFAULT NOW(),

    updated_at TIMESTAMPTZ
        NOT NULL
        DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS
idx_agents_last_seen
ON agents(last_seen DESC);

CREATE INDEX IF NOT EXISTS
idx_agents_state
ON agents(state, directive);

CREATE TABLE IF NOT EXISTS events (
    event_id BIGSERIAL PRIMARY KEY,

    event_type TEXT NOT NULL,

    agent_id UUID
        REFERENCES agents(agent_id)
        ON DELETE SET NULL,

    payload JSONB
        NOT NULL
        DEFAULT '{}'::jsonb,

    occurred_at TIMESTAMPTZ
        NOT NULL
        DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS
idx_deadman_events_time
ON events(occurred_at DESC);

CREATE INDEX IF NOT EXISTS
idx_deadman_events_agent
ON events(agent_id, occurred_at DESC);
