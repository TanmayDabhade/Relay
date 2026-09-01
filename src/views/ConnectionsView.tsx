import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { agentMeta } from "../lib/agents";
import { listAgentConnections, saveAgentConnection } from "../lib/tauri";
import type { AgentConnection } from "../lib/types";
import { Button } from "../components/ui/Button";
import "./ConnectionsView.css";

function ConnectionCard({ connection }: { connection: AgentConnection }) {
  const queryClient = useQueryClient();
  const meta = agentMeta(connection.agent);
  const [enabled, setEnabled] = useState(connection.enabled);
  const [executable, setExecutable] = useState(connection.executable);
  const [models, setModels] = useState(connection.models.join(", "));
  const [defaultModel, setDefaultModel] = useState(connection.default_model);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const parsedModels = models
    .split(",")
    .map((model) => model.trim())
    .filter(Boolean);
  const selectedDefault = parsedModels.includes(defaultModel)
    ? defaultModel
    : (parsedModels[0] ?? "");

  async function save() {
    setSaving(true);
    setMessage(null);
    try {
      await saveAgentConnection({
        agent: connection.agent,
        enabled,
        executable,
        models: parsedModels,
        defaultModel: selectedDefault,
      });
      await queryClient.invalidateQueries({ queryKey: ["agent-connections"] });
      setMessage("Saved");
    } catch (reason) {
      setMessage(String(reason));
    } finally {
      setSaving(false);
    }
  }

  return (
    <article className="connection-card">
      <div className="connection-card-heading">
        <div className="connection-identity">
          <span className="connection-agent-mark" aria-hidden="true">
            {meta.icon}
          </span>
          <div>
            <h2>{meta.label}</h2>
            <span className="connection-agent-id">{connection.agent}</span>
          </div>
        </div>
        <span className={`connection-status ${connection.installed ? "is-ready" : "is-missing"}`}>
          <span />
          {connection.installed ? "Detected" : "Not found"}
        </span>
      </div>

      <p className="connection-path">
        {connection.resolved_executable ?? "Set the CLI executable or an absolute path below."}
      </p>

      <div className="connection-fields">
        <label>
          <span>Executable</span>
          <input value={executable} onChange={(event) => setExecutable(event.target.value)} />
        </label>
        <label>
          <span>Configured models</span>
          <input
            value={models}
            onChange={(event) => setModels(event.target.value)}
            placeholder="default, model-id"
          />
          <small>Comma-separated model IDs passed directly to the CLI.</small>
        </label>
        <label>
          <span>Default model</span>
          <select
            value={selectedDefault}
            onChange={(event) => setDefaultModel(event.target.value)}
          >
            {parsedModels.map((model) => (
              <option key={model} value={model}>
                {model}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="connection-card-footer">
        <label className="connection-toggle">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(event) => setEnabled(event.target.checked)}
          />
          <span>Available for dispatch</span>
        </label>
        <div className="connection-save-group">
          {message ? <span className="connection-message">{message}</span> : null}
          <Button onClick={save} disabled={saving || parsedModels.length === 0}>
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>
    </article>
  );
}

export function ConnectionsView() {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["agent-connections"],
    queryFn: listAgentConnections,
  });

  return (
    <div className="connections-view">
      <header className="connections-header">
        <div>
          <span className="connections-eyebrow">Local runtime</span>
          <h1>Connections</h1>
          <p>Choose which built-in agent CLIs Relay may run and which models appear at dispatch.</p>
        </div>
        <div className="connections-privacy-note">
          <span>Credentials stay with each CLI</span>
          Relay stores executable and model names—not provider API keys.
        </div>
      </header>

      {isLoading ? <p className="connections-state">Detecting installed agents…</p> : null}
      {isError ? <p className="connections-state">Connections could not be loaded.</p> : null}
      {data ? (
        <div className="connections-grid">
          {data.map((connection) => (
            <ConnectionCard
              key={`${connection.agent}-${connection.updated_at}`}
              connection={connection}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}
