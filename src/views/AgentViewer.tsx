import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { agentMeta } from "../lib/agents";
import { isOngoingDispatch } from "../lib/format";
import {
  interruptDispatchTurn,
  stopDispatchLoop,
  listDispatchRuns,
  listDispatchTasks,
  retryDispatchTask,
  sendDispatchPrompt,
  shutdownDispatchConversation,
} from "../lib/tauri";
import type { DispatchRun, DispatchStatus } from "../lib/types";
import { Button } from "../components/ui/Button";
import { DispatchModal } from "../components/dispatch/DispatchModal";
import { RunChat } from "../components/dispatch/RunChat";
import { ShipPanel } from "../components/dispatch/ShipPanel";
import "./AgentViewer.css";

const TURN_IN_FLIGHT_STATUSES: DispatchStatus[] = [
  "queued",
  "starting",
  "running",
  "awaiting_approval",
  "interrupting",
  "shutting_down",
];
const PROMPTABLE_STATUSES: DispatchStatus[] = ["idle", "failed", "interrupted"];

interface AgentViewerProps {
  initialTaskId?: string;
  initialRunId?: string;
  initialStartedAt?: number;
  onOpenHome?: () => void;
  onTargetChange?: (target: {
    taskId: string;
    runId: string;
    startedAt: number;
  } | null) => void;
}

function localDateValue(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function dayBounds(value: string): [number, number] {
  const start = new Date(`${value}T00:00:00`);
  const end = new Date(start);
  end.setDate(end.getDate() + 1);
  return [Math.floor(start.getTime() / 1000), Math.floor(end.getTime() / 1000)];
}

function timeLabel(epoch: number): string {
  return new Date(epoch * 1000).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function statusLabel(status: DispatchStatus): string {
  const label = status.replaceAll("_", " ");
  return label.charAt(0).toUpperCase() + label.slice(1);
}

export function AgentViewer({
  initialTaskId,
  initialRunId,
  initialStartedAt,
  onOpenHome,
  onTargetChange,
}: AgentViewerProps) {
  const queryClient = useQueryClient();
  const today = localDateValue(new Date());
  const [date, setDate] = useState(
    initialStartedAt ? localDateValue(new Date(initialStartedAt * 1000)) : today,
  );
  const [dayStart, dayEnd] = useMemo(() => dayBounds(date), [date]);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(initialTaskId ?? null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(initialRunId ?? null);
  const [showComposer, setShowComposer] = useState(false);
  const [input, setInput] = useState("");
  const [actionError, setActionError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);

  const { data: entries = [], isLoading, isError } = useQuery({
    queryKey: ["dispatch-tasks", dayStart, dayEnd],
    queryFn: () => listDispatchTasks(dayStart, dayEnd),
  });
  const selectedEntry =
    entries.find((entry) => entry.run.id === selectedRunId) ??
    entries.find((entry) => entry.id === selectedTaskId) ??
    entries.find((entry) => isOngoingDispatch(entry.run.status)) ??
    entries[0] ??
    null;

  const { data: attempts = [] } = useQuery({
    queryKey: ["dispatch-runs", selectedEntry?.id],
    queryFn: () => listDispatchRuns(selectedEntry!.id),
    enabled: selectedEntry != null,
  });
  const selectedRun: DispatchRun | null =
    attempts.find((run) => run.id === selectedRunId) ?? selectedEntry?.run ?? null;

  const ongoingEntries = entries.filter((entry) => isOngoingDispatch(entry.run.status));
  const earlierEntries = entries.filter((entry) => !isOngoingDispatch(entry.run.status));
  const activeCount = ongoingEntries.length;
  const projectCount = new Set(entries.map((entry) => entry.project_id)).size;
  const hasActiveTurn = selectedRun != null && TURN_IN_FLIGHT_STATUSES.includes(selectedRun.status);
  const canPrompt = selectedRun != null && PROMPTABLE_STATUSES.includes(selectedRun.status);
  const isShutDown = selectedRun?.status === "shut_down";
  const taskHasActiveAttempt = attempts.some((attempt) => isOngoingDispatch(attempt.status));

  async function sendInput(event: React.FormEvent) {
    event.preventDefault();
    if (!selectedRun || !input.trim()) return;
    setBusyAction("input");
    setActionError(null);
    try {
      await sendDispatchPrompt(selectedRun.id, input);
      setInput("");
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["dispatch-conversation", selectedRun.id] }),
        queryClient.invalidateQueries({ queryKey: ["dispatch-tasks"] }),
      ]);
    } catch (reason) {
      setActionError(String(reason));
    } finally {
      setBusyAction(null);
    }
  }

  async function interruptTurn() {
    if (!selectedRun) return;
    setBusyAction("cancel");
    setActionError(null);
    try {
      await interruptDispatchTurn(selectedRun.id);
    } catch (reason) {
      setActionError(String(reason));
    } finally {
      setBusyAction(null);
    }
  }

  async function stopLoop() {
    if (!selectedRun) return;
    setBusyAction("stop-loop");
    setActionError(null);
    try {
      await stopDispatchLoop(selectedRun.id);
      await queryClient.invalidateQueries({ queryKey: ["dispatch-tasks"] });
      await queryClient.invalidateQueries({ queryKey: ["dispatch-runs"] });
    } catch (reason) {
      setActionError(String(reason));
    } finally {
      setBusyAction(null);
    }
  }

  async function shutDownConversation() {
    if (!selectedRun) return;
    if (!window.confirm("Shut down this conversation? You will not be able to send more prompts.")) {
      return;
    }
    setBusyAction("shutdown");
    setActionError(null);
    try {
      await shutdownDispatchConversation(selectedRun.id);
    } catch (reason) {
      setActionError(String(reason));
    } finally {
      setBusyAction(null);
    }
  }

  async function retryRun() {
    if (!selectedEntry || !selectedRun) return;
    setBusyAction("retry");
    setActionError(null);
    try {
      const created = await retryDispatchTask(
        selectedEntry.id,
        selectedRun.agent,
        selectedRun.model,
      );
      setDate(today);
      setSelectedTaskId(created.task.id);
      setSelectedRunId(created.run.id);
      onTargetChange?.({
        taskId: created.task.id,
        runId: created.run.id,
        startedAt: created.run.started_at ?? created.run.created_at,
      });
      await queryClient.invalidateQueries({ queryKey: ["dispatch-tasks"] });
    } catch (reason) {
      setActionError(String(reason));
    } finally {
      setBusyAction(null);
    }
  }

  return (
    <div className="agent-viewer">
      <header className="agent-viewer-header">
        <div className="agent-viewer-heading">
          {onOpenHome ? (
            <Button variant="secondary" className="agent-viewer-back" onClick={onOpenHome}>
              <span aria-hidden>←</span> Home
            </Button>
          ) : null}
          <div>
            <span className="agent-viewer-eyebrow">Live Relay workspace</span>
            <h1>Agent work</h1>
          </div>
        </div>
        <div className="agent-viewer-actions">
          <input
            className="agent-viewer-date"
            type="date"
            value={date}
            onChange={(event) => {
              if (!event.target.value) return;
              setDate(event.target.value);
              setSelectedTaskId(null);
              setSelectedRunId(null);
              onTargetChange?.(null);
            }}
            aria-label="Workday"
          />
          {date !== today ? (
            <Button
              variant="secondary"
              onClick={() => {
                setDate(today);
                setSelectedTaskId(null);
                setSelectedRunId(null);
                onTargetChange?.(null);
              }}
            >
              Today
            </Button>
          ) : null}
          <Button onClick={() => setShowComposer(true)}>+ New run</Button>
        </div>
      </header>

      <div className="agent-viewer-workspace">
        <aside className="workday-rail" aria-label="Runs for selected workday">
          <div className="workday-rail-heading">
            <div>
              <strong>{date === today ? "Today" : date}</strong>
              <span>{entries.length} runs · {projectCount} projects</span>
            </div>
            <span>{activeCount} ongoing</span>
          </div>
          {isLoading ? <p className="workday-empty">Loading runs…</p> : null}
          {isError ? <p className="workday-empty">The workday could not be loaded.</p> : null}
          {!isLoading && !isError && entries.length === 0 ? (
            <div className="workday-empty">
              <span className="workday-empty-mark">○</span>
              <p>No Relay-owned runs on this day.</p>
              <button onClick={() => setShowComposer(true)}>Start the first one</button>
            </div>
          ) : null}
          {[
            { label: "Ongoing", runs: ongoingEntries },
            { label: "Earlier", runs: earlierEntries },
          ].map((group) => group.runs.length > 0 ? (
            <section className="workday-run-group" key={group.label}>
              <h2>{group.label}</h2>
              <div className="workday-run-list">
                {group.runs.map((entry) => {
                  const meta = agentMeta(entry.run.agent);
                  const selected = entry.run.id === selectedRun?.id;
                  return (
                    <button
                      key={entry.run.id}
                      className={`workday-run${selected ? " is-selected" : ""}`}
                      aria-pressed={selected}
                      onClick={() => {
                        setSelectedTaskId(entry.id);
                        setSelectedRunId(entry.run.id);
                        setActionError(null);
                        onTargetChange?.({
                          taskId: entry.id,
                          runId: entry.run.id,
                          startedAt: entry.run.started_at ?? entry.run.created_at,
                        });
                      }}
                    >
                      <span className="workday-time">
                        {timeLabel(entry.run.started_at ?? entry.run.created_at)}
                      </span>
                      <span className={`run-status-dot status-${entry.run.status}`} />
                      <span className="workday-run-copy">
                        <strong>{entry.title}</strong>
                        <span>{entry.project_name}</span>
                        <small>
                          {meta.label} · {entry.run.model} · {statusLabel(entry.run.status)}
                        </small>
                      </span>
                    </button>
                  );
                })}
              </div>
            </section>
          ) : null)}
        </aside>

        <section className="run-inspector">
          {!selectedEntry || !selectedRun ? (
            <div className="run-inspector-empty">
              <span>Relay / no run selected</span>
              <p>Select a run from the workday rail to inspect its agent conversation.</p>
            </div>
          ) : (
            <div className="run-inspector-session" key={selectedRun.id}>
              <div className="run-inspector-heading">
                <div className="run-inspector-title">
                  <span className={`run-status-label status-${selectedRun.status}`}>
                    {statusLabel(selectedRun.status)}
                  </span>
                  <h2>{selectedEntry.title}</h2>
                  <p title={selectedEntry.project_path}>{selectedEntry.project_name} · {selectedEntry.project_path}</p>
                </div>
                <div className="run-inspector-controls">
                  {attempts.length > 1 ? (
                    <select
                      value={selectedRun.id}
                      onChange={(event) => {
                        const runId = event.target.value;
                        const attempt = attempts.find((candidate) => candidate.id === runId);
                        setSelectedRunId(runId);
                        if (attempt) {
                          onTargetChange?.({
                            taskId: selectedEntry.id,
                            runId,
                            startedAt: attempt.started_at ?? attempt.created_at,
                          });
                        }
                      }}
                      aria-label="Run attempt"
                    >
                      {[...attempts].reverse().map((attempt) => (
                        <option key={attempt.id} value={attempt.id}>
                          Attempt {attempt.attempt} · {statusLabel(attempt.status)}
                        </option>
                      ))}
                    </select>
                  ) : null}
                  {(["completed", "cancelled", "shut_down"] as DispatchStatus[]).includes(
                    selectedRun.status,
                  ) ? (
                    <Button
                      variant="secondary"
                      onClick={retryRun}
                      disabled={busyAction != null || taskHasActiveAttempt}
                    >
                      {busyAction === "retry"
                        ? "Starting…"
                        : taskHasActiveAttempt
                          ? "Run active"
                          : "Retry"}
                    </Button>
                  ) : null}
                </div>
              </div>

              <div className="run-command-strip">
                <span>{agentMeta(selectedRun.agent).icon} {agentMeta(selectedRun.agent).label}</span>
                <span>{selectedRun.model}</span>
                <span>attempt {selectedRun.attempt}</span>
                {selectedRun.session_id ? <span>session {selectedRun.session_id.slice(0, 8)}</span> : null}
                {selectedRun.loop_max_iterations != null ? (
                  <span className="run-loop-chip">
                    loop {selectedRun.loop_iterations}/{selectedRun.loop_max_iterations}
                    {!isShutDown ? (
                      <button
                        type="button"
                        onClick={stopLoop}
                        disabled={busyAction != null}
                        title="Let the current turn finish, then stop continuing"
                      >
                        {busyAction === "stop-loop" ? "Stopping…" : "Stop loop"}
                      </button>
                    ) : null}
                  </span>
                ) : null}
              </div>

              <ShipPanel taskId={selectedEntry.id} busy={hasActiveTurn} />

              <div className="run-console">
                <RunChat runId={selectedRun.id} active={hasActiveTurn} />
              </div>

              {selectedRun.error ? <p className="run-action-error">{selectedRun.error}</p> : null}
              {actionError ? <p className="run-action-error">{actionError}</p> : null}

              <form className="run-input" onSubmit={sendInput}>
                <textarea
                  value={input}
                  onChange={(event) => setInput(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                      event.currentTarget.form?.requestSubmit();
                    }
                  }}
                  rows={2}
                  placeholder={
                    canPrompt
                      ? "Continue this conversation…"
                      : isShutDown
                        ? "This conversation has been shut down"
                        : "Wait for the current turn, or stop it to redirect the agent…"
                  }
                  disabled={!canPrompt || busyAction != null}
                />
                {hasActiveTurn && selectedRun.status !== "shutting_down" ? (
                  <Button
                    type="button"
                    variant="secondary"
                    className="run-stop"
                    onClick={interruptTurn}
                    disabled={busyAction != null}
                  >
                    {busyAction === "cancel" ? "Stopping…" : "Stop turn"}
                  </Button>
                ) : null}
                {!isShutDown ? (
                  <Button
                    type="button"
                    variant="secondary"
                    className="run-shutdown"
                    onClick={shutDownConversation}
                    disabled={busyAction != null}
                  >
                    {busyAction === "shutdown" ? "Shutting down…" : "Shut down"}
                  </Button>
                ) : null}
                <Button type="submit" disabled={!canPrompt || !input.trim() || busyAction != null}>
                  {busyAction === "input" ? "Sending…" : "Send"}
                </Button>
              </form>
            </div>
          )}
        </section>
      </div>

      {showComposer ? (
        <DispatchModal
          onClose={() => setShowComposer(false)}
          onDispatched={(created) => {
            setDate(today);
            setSelectedTaskId(created.task.id);
            setSelectedRunId(created.run.id);
            onTargetChange?.({
              taskId: created.task.id,
              runId: created.run.id,
              startedAt: created.run.started_at ?? created.run.created_at,
            });
          }}
        />
      ) : null}
    </div>
  );
}
