import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { dispatchTask, listAgentConnections, listProjects } from "../../lib/tauri";
import type { Card, CreatedDispatch } from "../../lib/types";
import { Button } from "../ui/Button";
import { Modal } from "../ui/Modal";
import "./DispatchModal.css";

/** Mirrors `looping::MAX_LOOP_ITERATIONS` in the backend, which rejects anything larger. */
const MAX_LOOP_ITERATIONS = 50;

interface DispatchModalProps {
  projectId?: string;
  card?: Card | null;
  onClose: () => void;
  onDispatched?: (dispatch: CreatedDispatch) => void;
}

export function DispatchModal({ projectId, card, onClose, onDispatched }: DispatchModalProps) {
  const queryClient = useQueryClient();
  const { data: connections = [], isLoading } = useQuery({
    queryKey: ["agent-connections"],
    queryFn: listAgentConnections,
  });
  const { data: projects = [], isLoading: projectsLoading } = useQuery({
    queryKey: ["projects"],
    queryFn: listProjects,
    enabled: projectId == null,
  });
  const [projectOverride, setProjectOverride] = useState("");
  const selectedProjectId = projectId ?? (projectOverride || projects[0]?.id || "");
  const available = useMemo(
    () => connections.filter((connection) => connection.enabled && connection.installed),
    [connections],
  );
  const [agent, setAgent] = useState<string>("");
  const selectedConnection =
    available.find((connection) => connection.agent === agent) ?? available[0];
  const [modelOverride, setModelOverride] = useState<string>("");
  const model =
    selectedConnection?.models.includes(modelOverride) === true
      ? modelOverride
      : selectedConnection?.default_model ?? "";
  const [title, setTitle] = useState(card?.title ?? "");
  const [prompt, setPrompt] = useState(card?.description ?? card?.title ?? "");
  const [loopEnabled, setLoopEnabled] = useState(false);
  const [loopMax, setLoopMax] = useState(5);
  const loopMaxValid = Number.isInteger(loopMax) && loopMax >= 1 && loopMax <= MAX_LOOP_ITERATIONS;
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!selectedConnection) return;
    setSubmitting(true);
    setError(null);
    try {
      const created = await dispatchTask({
        projectId: selectedProjectId,
        cardId: card?.id,
        title,
        prompt,
        agent: selectedConnection.agent,
        model,
        loopMaxIterations: loopEnabled ? loopMax : null,
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["board", selectedProjectId] }),
        queryClient.invalidateQueries({ queryKey: ["dispatch-tasks"] }),
      ]);
      onDispatched?.(created);
      onClose();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Modal isOpen onClose={onClose} title={card ? "Dispatch card" : "New agent task"}>
      <form className="dispatch-form" onSubmit={submit}>
        <div className="dispatch-form-intro">
          <span className="dispatch-form-kicker">Relay-owned run</span>
          <p>
            Relay starts the selected local CLI in this project and keeps its output,
            follow-ups, and final state in your workday history.
          </p>
        </div>

        {!projectId ? (
          <label className="dispatch-field">
            <span>Project</span>
            <select
              value={selectedProjectId}
              onChange={(event) => setProjectOverride(event.target.value)}
            >
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name} — {project.path}
                </option>
              ))}
            </select>
            {projectsLoading ? <small>Loading projects…</small> : null}
            {!projectsLoading && projects.length === 0 ? (
              <small>No indexed project is available yet.</small>
            ) : null}
          </label>
        ) : null}

        <label className="dispatch-field">
          <span>Task title</span>
          <input value={title} onChange={(event) => setTitle(event.target.value)} autoFocus />
        </label>

        <label className="dispatch-field">
          <span>Instructions</span>
          <textarea
            value={prompt}
            onChange={(event) => setPrompt(event.target.value)}
            rows={7}
            placeholder="Describe the outcome you want the agent to produce…"
          />
        </label>

        {isLoading ? <p className="dispatch-form-note">Checking local agents…</p> : null}
        {!isLoading && available.length === 0 ? (
          <p className="dispatch-form-error">
            No enabled built-in agent was found. Configure an executable in Connections first.
          </p>
        ) : null}

        {selectedConnection ? (
          <div className="dispatch-agent-row">
            <label className="dispatch-field">
              <span>Agent</span>
              <select
                value={selectedConnection.agent}
                onChange={(event) => {
                  setAgent(event.target.value);
                  setModelOverride("");
                }}
              >
                {available.map((connection) => (
                  <option key={connection.agent} value={connection.agent}>
                    {connection.agent}
                  </option>
                ))}
              </select>
            </label>
            <label className="dispatch-field">
              <span>Model</span>
              <select value={model} onChange={(event) => setModelOverride(event.target.value)}>
                {selectedConnection.models.map((configuredModel) => (
                  <option key={configuredModel} value={configuredModel}>
                    {configuredModel}
                  </option>
                ))}
              </select>
            </label>
          </div>
        ) : null}

        <div className="dispatch-loop">
          <label className="dispatch-loop-toggle">
            <input
              type="checkbox"
              checked={loopEnabled}
              onChange={(event) => setLoopEnabled(event.target.checked)}
            />
            <span>
              <strong>Loop until done</strong>
              <small>
                After each turn, Relay tells the agent to keep going until it reports the task
                complete. Each turn costs money, so set a limit.
              </small>
            </span>
          </label>
          {loopEnabled ? (
            <label className="dispatch-field dispatch-loop-max">
              <span>Max continuations</span>
              <input
                type="number"
                min={1}
                max={MAX_LOOP_ITERATIONS}
                value={Number.isNaN(loopMax) ? "" : loopMax}
                onChange={(event) => setLoopMax(event.target.valueAsNumber)}
              />
              {!loopMaxValid ? <small>Enter 1–{MAX_LOOP_ITERATIONS}.</small> : null}
            </label>
          ) : null}
        </div>

        {error ? <p className="dispatch-form-error">{error}</p> : null}

        <div className="dispatch-form-actions">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            disabled={
              submitting ||
              !selectedConnection ||
              !selectedProjectId ||
              !title.trim() ||
              !prompt.trim() ||
              !model ||
              (loopEnabled && !loopMaxValid)
            }
          >
            {submitting ? "Starting…" : "Start run"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
