import React, {
  useEffect,
  useMemo,
  useState
} from "react";
import { createRoot } from "react-dom/client";
import "./style.css";

type Policy = {
  heartbeat_timeout_secs: number;
  hard_timeout_secs: number;
  max_consecutive_failures: number;
  fail_closed: boolean;
};

type Agent = {
  agent_id: string;
  name: string;
  platform: string;
  arch: string;
  version: string;
  state:
    | "healthy"
    | "suspect"
    | "tripped"
    | "contained"
    | "recovering"
    | "offline";
  directive:
    | "run"
    | "contain"
    | "stop";
  armed: boolean;
  workload_running: boolean;
  workload_pid?: number;
  consecutive_failures: number;
  last_seen: string;
  policy: Policy;
  metadata: Record<string, unknown>;
};

type EventView = {
  event_id: number;
  event_type: string;
  agent_id?: string;
  payload: unknown;
  occurred_at: string;
};

const apiBase =
  import.meta.env.VITE_API_URL ||
  `${window.location.protocol}//${window.location.hostname}:8794`;

async function request<T>(
  token: string,
  path: string,
  init: RequestInit = {}
): Promise<T> {
  const response = await fetch(
    `${apiBase}${path}`,
    {
      ...init,
      headers: {
        "Content-Type":
          "application/json",
        Authorization:
          `Bearer ${token}`,
        ...(init.headers || {})
      }
    }
  );

  if (!response.ok) {
    throw new Error(
      `${response.status} ${await response.text()}`
    );
  }

  return response.json() as Promise<T>;
}

function age(timestamp: string): string {
  const seconds =
    Math.max(
      0,
      Math.floor(
        (
          Date.now() -
          new Date(timestamp)
            .getTime()
        ) / 1000
      )
    );

  if (seconds < 60) {
    return `${seconds}s`;
  }

  return `${Math.floor(
    seconds / 60
  )}m`;
}

function App() {
  const [token, setToken] =
    useState(
      sessionStorage.getItem(
        "deadman_admin_token"
      ) || ""
    );

  const [
    connected,
    setConnected
  ] = useState(
    Boolean(
      sessionStorage.getItem(
        "deadman_admin_token"
      )
    )
  );

  const [agents, setAgents] =
    useState<Agent[]>([]);

  const [
    selected,
    setSelected
  ] = useState<Agent | null>(null);

  const [events, setEvents] =
    useState<EventView[]>([]);

  const [error, setError] =
    useState("");

  const [
    heartbeatTimeout,
    setHeartbeatTimeout
  ] = useState("15");

  const [
    hardTimeout,
    setHardTimeout
  ] = useState("30");

  const [
    maxFailures,
    setMaxFailures
  ] = useState("3");

  const [
    failClosed,
    setFailClosed
  ] = useState(true);

  const healthy =
    useMemo(
      () =>
        agents.filter(
          (agent) =>
            agent.state ===
            "healthy"
        ).length,
      [agents]
    );

  const contained =
    useMemo(
      () =>
        agents.filter(
          (agent) =>
            agent.directive ===
              "contain" ||
            agent.directive ===
              "stop"
        ).length,
      [agents]
    );

  const tripped =
    useMemo(
      () =>
        agents.filter(
          (agent) =>
            agent.state ===
              "tripped" ||
            agent.state ===
              "contained"
        ).length,
      [agents]
    );

  async function refresh() {
    if (!token) return;

    try {
      const [
        nextAgents,
        nextEvents
      ] = await Promise.all([
        request<Agent[]>(
          token,
          "/api/v1/agents"
        ),

        request<EventView[]>(
          token,
          "/api/v1/events?limit=120"
        )
      ]);

      setAgents(nextAgents);
      setEvents(nextEvents);

      if (selected) {
        const fresh =
          nextAgents.find(
            (agent) =>
              agent.agent_id ===
              selected.agent_id
          );

        if (fresh) {
          setSelected(fresh);
        }
      }

      setError("");
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : String(err)
      );
    }
  }

  useEffect(() => {
    if (!connected) return;

    refresh();

    const timer =
      window.setInterval(
        refresh,
        2500
      );

    return () =>
      window.clearInterval(timer);
  }, [
    connected,
    token,
    selected?.agent_id
  ]);

  function choose(
    agent: Agent
  ) {
    setSelected(agent);

    setHeartbeatTimeout(
      String(
        agent.policy
          .heartbeat_timeout_secs
      )
    );

    setHardTimeout(
      String(
        agent.policy
          .hard_timeout_secs
      )
    );

    setMaxFailures(
      String(
        agent.policy
          .max_consecutive_failures
      )
    );

    setFailClosed(
      agent.policy.fail_closed
    );
  }

  async function connect() {
    sessionStorage.setItem(
      "deadman_admin_token",
      token
    );

    setConnected(true);
  }

  async function directive(
    value:
      | "run"
      | "contain"
      | "stop"
  ) {
    if (!selected) return;

    try {
      await request(
        token,
        `/api/v1/agents/${selected.agent_id}/directive`,
        {
          method: "POST",
          body: JSON.stringify({
            directive: value,
            reason:
              "operator directive"
          })
        }
      );

      await refresh();
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : String(err)
      );
    }
  }

  async function arm(
    armed: boolean
  ) {
    if (!selected) return;

    try {
      await request(
        token,
        `/api/v1/agents/${selected.agent_id}/arm`,
        {
          method: "POST",
          body: JSON.stringify({
            armed
          })
        }
      );

      await refresh();
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : String(err)
      );
    }
  }

  async function savePolicy() {
    if (!selected) return;

    try {
      await request(
        token,
        `/api/v1/agents/${selected.agent_id}/policy`,
        {
          method: "POST",
          body: JSON.stringify({
            heartbeat_timeout_secs:
              Number(
                heartbeatTimeout
              ),
            hard_timeout_secs:
              Number(
                hardTimeout
              ),
            max_consecutive_failures:
              Number(
                maxFailures
              ),
            fail_closed:
              failClosed
          })
        }
      );

      await refresh();
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : String(err)
      );
    }
  }

  if (!connected) {
    return (
      <main className="login">
        <section className="login-card">
          <div className="mark">
            ":"
          </div>

          <div className="eyebrow">
            TITAN UNIVERSAL AI
          </div>

          <h1>
            Agent
            <br />
            Deadman Switch
          </h1>

          <p>
            Deterministic containment
            control.
          </p>

          <input
            type="password"
            autoFocus
            placeholder="Admin token"
            value={token}
            onChange={(event) =>
              setToken(
                event.target.value
              )
            }
          />

          <button
            disabled={!token}
            onClick={connect}
          >
            ENTER
          </button>
        </section>
      </main>
    );
  }

  return (
    <main className="shell">
      <header>
        <div>
          <div className="eyebrow">
            TITAN UNIVERSAL AI
          </div>

          <h1>
            Agent Deadman Switch
          </h1>
        </div>

        <div className="header-state">
          <span className="live" />
          WATCHDOG ACTIVE
          <strong>":"</strong>
        </div>
      </header>

      {error && (
        <div className="error">
          {error}
        </div>
      )}

      <section className="metrics">
        <article>
          <span>AGENTS</span>
          <strong>
            {agents.length}
          </strong>
        </article>

        <article>
          <span>HEALTHY</span>
          <strong>
            {healthy}
          </strong>
        </article>

        <article>
          <span>TRIPPED</span>
          <strong>
            {tripped}
          </strong>
        </article>

        <article>
          <span>CONTAINED</span>
          <strong>
            {contained}
          </strong>
        </article>
      </section>

      <section className="workspace">
        <aside className="panel agents">
          <div className="panel-title">
            <h2>
              Supervised Agents
            </h2>

            <span>
              {agents.length}
            </span>
          </div>

          <div className="agent-list">
            {!agents.length && (
              <div className="empty">
                Waiting for
                watchdog agents.
              </div>
            )}

            {agents.map(
              (agent) => (
                <button
                  key={
                    agent.agent_id
                  }
                  onClick={() =>
                    choose(agent)
                  }
                  className={
                    selected
                      ?.agent_id ===
                    agent.agent_id
                      ? "agent selected"
                      : "agent"
                  }
                >
                  <div className="agent-head">
                    <strong>
                      {agent.name}
                    </strong>

                    <span
                      className={
                        `state ${agent.state}`
                      }
                    >
                      {
                        agent.state
                      }
                    </span>
                  </div>

                  <div className="agent-meta">
                    {agent.platform}
                    {" / "}
                    {agent.arch}
                  </div>

                  <div className="agent-meta">
                    seen{" "}
                    {age(
                      agent.last_seen
                    )}{" "}
                    ago
                  </div>

                  <div className="agent-meta">
                    directive:{" "}
                    {
                      agent.directive
                    }
                  </div>
                </button>
              )
            )}
          </div>
        </aside>

        <section className="panel control">
          <div className="panel-title">
            <h2>
              Deadman Control
            </h2>

            <span>
              {selected?.name ||
                "NO AGENT"}
            </span>
          </div>

          {!selected ? (
            <div className="empty large">
              Select an agent.
            </div>
          ) : (
            <>
              <div className="identity-grid">
                <article>
                  <span>STATE</span>
                  <strong>
                    {
                      selected.state
                    }
                  </strong>
                </article>

                <article>
                  <span>
                    DIRECTIVE
                  </span>
                  <strong>
                    {
                      selected.directive
                    }
                  </strong>
                </article>

                <article>
                  <span>ARMED</span>
                  <strong>
                    {selected.armed
                      ? "YES"
                      : "NO"}
                  </strong>
                </article>

                <article>
                  <span>
                    WORKLOAD
                  </span>
                  <strong>
                    {selected
                      .workload_running
                      ? "RUNNING"
                      : "STOPPED"}
                  </strong>
                </article>
              </div>

              <div className="command-grid">
                <button
                  className="run"
                  onClick={() =>
                    directive(
                      "run"
                    )
                  }
                >
                  RUN
                </button>

                <button
                  className="contain"
                  onClick={() =>
                    directive(
                      "contain"
                    )
                  }
                >
                  CONTAIN
                </button>

                <button
                  className="stop"
                  onClick={() =>
                    directive(
                      "stop"
                    )
                  }
                >
                  STOP
                </button>
              </div>

              <div className="arm-grid">
                <button
                  onClick={() =>
                    arm(true)
                  }
                >
                  ARM
                </button>

                <button
                  onClick={() =>
                    arm(false)
                  }
                >
                  DISARM
                </button>
              </div>

              <div className="policy">
                <h3>
                  Deterministic Policy
                </h3>

                <label>
                  HEARTBEAT WARNING
                  <input
                    type="number"
                    value={
                      heartbeatTimeout
                    }
                    onChange={(e) =>
                      setHeartbeatTimeout(
                        e.target.value
                      )
                    }
                  />
                </label>

                <label>
                  HARD TIMEOUT
                  <input
                    type="number"
                    value={
                      hardTimeout
                    }
                    onChange={(e) =>
                      setHardTimeout(
                        e.target.value
                      )
                    }
                  />
                </label>

                <label>
                  MAX FAILURES
                  <input
                    type="number"
                    value={
                      maxFailures
                    }
                    onChange={(e) =>
                      setMaxFailures(
                        e.target.value
                      )
                    }
                  />
                </label>

                <label className="check">
                  <input
                    type="checkbox"
                    checked={
                      failClosed
                    }
                    onChange={(e) =>
                      setFailClosed(
                        e.target.checked
                      )
                    }
                  />

                  FAIL CLOSED
                </label>

                <button
                  className="save"
                  onClick={
                    savePolicy
                  }
                >
                  SAVE POLICY
                </button>
              </div>

              <div className="details">
                <div>
                  <span>ID</span>
                  <code>
                    {
                      selected.agent_id
                    }
                  </code>
                </div>

                <div>
                  <span>PID</span>
                  <code>
                    {selected
                      .workload_pid ??
                      "—"}
                  </code>
                </div>

                <div>
                  <span>FAILURES</span>
                  <code>
                    {
                      selected
                        .consecutive_failures
                    }
                  </code>
                </div>
              </div>
            </>
          )}
        </section>

        <aside className="panel event-panel">
          <div className="panel-title">
            <h2>
              Control Events
            </h2>

            <span>
              POSTGRES + KAFKA
            </span>
          </div>

          <div className="events">
            {events.map(
              (event) => (
                <article
                  key={
                    event.event_id
                  }
                >
                  <strong>
                    {
                      event.event_type
                    }
                  </strong>

                  <span>
                    {event.agent_id
                      ? event.agent_id
                          .slice(
                            0,
                            8
                          )
                      : "system"}
                  </span>

                  <time>
                    {new Date(
                      event.occurred_at
                    ).toLocaleTimeString()}
                  </time>
                </article>
              )
            )}
          </div>
        </aside>
      </section>
    </main>
  );
}

createRoot(
  document.getElementById(
    "root"
  )!
).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
