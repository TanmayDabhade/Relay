import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { DispatchModal } from "../components/dispatch/DispatchModal";
import { Button } from "../components/ui/Button";
import { ActivityHeatmap } from "../components/ui/ActivityHeatmap";
import { ProjectDot } from "../components/ui/ProjectDot";
import { StatTile } from "../components/ui/StatTile";
import { agentMeta, KNOWN_AGENT_IDS } from "../lib/agents";
import { formatRelativeTime, isOngoingDispatch } from "../lib/format";
import { colorForProject } from "../lib/projectColor";
import { getDashboardStats, listDispatchTasks } from "../lib/tauri";
import type { AgentUsage, DispatchStatus } from "../lib/types";
import "./DashboardView.css";

const STATUS_COPY: Record<DispatchStatus, string> = {
  queued: "Queued",
  starting: "Starting",
  running: "Working",
  awaiting_approval: "Needs approval",
  interrupting: "Stopping",
  idle: "Ready for follow-up",
  shutting_down: "Shutting down",
  shut_down: "Shut down",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
  interrupted: "Interrupted",
};

interface DashboardViewProps {
  onOpenAgentWork: (target?: {
    taskId: string;
    runId: string;
    startedAt: number;
  }) => void;
}

export function DashboardView({ onOpenAgentWork }: DashboardViewProps) {
  const [showComposer, setShowComposer] = useState(false);
  const {
    data,
    isLoading: statsLoading,
    isError: statsError,
  } = useQuery({
    queryKey: ["dashboard"],
    queryFn: getDashboardStats,
  });
  const {
    data: relayEntries = [],
    isLoading: relayLoading,
    isError: relayError,
  } = useQuery({
    queryKey: ["dispatch-tasks", "ongoing"],
    queryFn: () => listDispatchTasks(0, Math.floor(Date.now() / 1000) + 86_400),
  });

  const ongoingEntries = relayEntries.filter((entry) => isOngoingDispatch(entry.run.status));
  const visibleEntries = ongoingEntries.slice(0, 6);
  const usageByAgent = new Map<string, AgentUsage>(
    data?.agent_usage.map((usage) => [usage.agent, usage]) ?? [],
  );
  const topProject = data?.top_projects[0];

  return (
    <div className="dashboard-view">
      <header className="home-launcher">
        <div className="home-launcher-copy">
          <span className="home-live-label"><i aria-hidden /> Relay is ready</span>
          <h1>Start with Relay.</h1>
          <p>
            Launch a coding agent here, then keep its progress, approvals, and follow-ups
            in one place.
          </p>
        </div>
        <Button className="home-launch-button" onClick={() => setShowComposer(true)}>
          Start an agent task <span aria-hidden>→</span>
        </Button>
      </header>

      <section className="home-work-section" aria-labelledby="ongoing-work-heading">
        <div className="home-section-heading">
          <div>
            <h2 id="ongoing-work-heading">Ongoing work</h2>
            <span>
              {relayLoading
                ? "Checking Relay tasks…"
                : `${ongoingEntries.length} conversation${ongoingEntries.length === 1 ? "" : "s"} in motion`}
            </span>
          </div>
          <button className="home-text-action" onClick={() => onOpenAgentWork()}>
            View all agent work <span aria-hidden>→</span>
          </button>
        </div>

        <div className="home-work-ledger">
          {relayLoading ? (
            <div className="home-work-loading" role="status">
              <span aria-hidden><i /><i /><i /></span>
              Loading ongoing work…
            </div>
          ) : null}
          {relayError ? (
            <p className="home-work-message">Relay tasks couldn’t be loaded.</p>
          ) : null}
          {!relayLoading && !relayError && ongoingEntries.length === 0 ? (
            <div className="home-work-empty">
              <span className="home-empty-node" aria-hidden>○</span>
              <div>
                <strong>No agent work is running.</strong>
                <p>Start a task here and Relay will keep the conversation ready for you.</p>
              </div>
              <Button variant="secondary" onClick={() => setShowComposer(true)}>
                Start the first task
              </Button>
            </div>
          ) : null}

          {visibleEntries.map((entry) => {
            const meta = agentMeta(entry.run.agent);
            const startedAt = entry.run.started_at ?? entry.run.created_at;
            return (
              <button
                key={entry.run.id}
                className="home-task-row"
                onClick={() =>
                  onOpenAgentWork({
                    taskId: entry.id,
                    runId: entry.run.id,
                    startedAt,
                  })
                }
                aria-label={`Open ${entry.title} in Agent work`}
              >
                <span className={`home-task-node status-${entry.run.status}`} aria-hidden>
                  <i />
                </span>
                <span className="home-task-copy">
                  <span className="home-task-title-line">
                    <strong>{entry.title}</strong>
                    <span className={`home-task-status status-${entry.run.status}`}>
                      {STATUS_COPY[entry.run.status]}
                    </span>
                  </span>
                  <span className="home-task-meta">
                    <span><ProjectDot projectId={entry.project_id} /> {entry.project_name}</span>
                    <span>{meta.icon} {meta.label}</span>
                    <span>{entry.run.model}</span>
                  </span>
                </span>
                <time dateTime={new Date(entry.updated_at * 1000).toISOString()}>
                  {formatRelativeTime(entry.updated_at)}
                </time>
                <span className="home-task-arrow" aria-hidden>→</span>
              </button>
            );
          })}

          {ongoingEntries.length > visibleEntries.length ? (
            <button className="home-ledger-more" onClick={() => onOpenAgentWork()}>
              {ongoingEntries.length - visibleEntries.length} more ongoing
            </button>
          ) : null}
        </div>
      </section>

      <section className="home-insights" aria-labelledby="workspace-pulse-heading">
        <div className="home-section-heading">
          <div>
            <h2 id="workspace-pulse-heading">Workspace pulse</h2>
            <span>Your local agent activity at a glance</span>
          </div>
        </div>

        {statsLoading ? <p className="dashboard-view-status">Loading workspace activity…</p> : null}
        {statsError ? (
          <p className="dashboard-view-status">Workspace activity couldn’t be loaded.</p>
        ) : null}
        {data ? (
          <>
            <div className="dashboard-stats-row">
              <StatTile value={`$${data.total_cost_usd.toFixed(2)}`} label="Total spend" />
              <StatTile value={data.total_sessions} label="Total sessions" />
              <StatTile value={data.total_projects} label="Indexed projects" />
              <StatTile
                value={topProject ? topProject.name : "—"}
                label="Highest usage project"
              />
            </div>

            <section className="dashboard-section">
              <h3 className="dashboard-section-title">Activity</h3>
              <div className="dashboard-card">
                <ActivityHeatmap data={data.daily_activity} />
              </div>
            </section>

            <div className="dashboard-columns">
              <section className="dashboard-section dashboard-column">
                <h3 className="dashboard-section-title">Highest usage projects</h3>
                <div className="dashboard-card dashboard-project-list">
                  {data.top_projects.length === 0 && (
                    <p className="dashboard-view-status">No projects yet.</p>
                  )}
                  {data.top_projects.map((project, index) => {
                    const maxCost = data.top_projects[0]?.total_cost_usd || 1;
                    const percentage = maxCost > 0
                      ? (project.total_cost_usd / maxCost) * 100
                      : 0;
                    return (
                      <div className="dashboard-project-row" key={project.id}>
                        <span className="dashboard-project-rank">{index + 1}</span>
                        <div className="dashboard-project-info">
                          <div className="dashboard-project-top">
                            <span className="dashboard-project-name">
                              <ProjectDot projectId={project.id} />
                              {project.name}
                            </span>
                            <span className="dashboard-project-cost">
                              ${project.total_cost_usd.toFixed(2)}
                            </span>
                          </div>
                          <div className="dashboard-project-bar-track">
                            <div
                              className="dashboard-project-bar-fill"
                              style={{
                                width: `${Math.max(percentage, 3)}%`,
                                backgroundColor: colorForProject(project.id),
                              }}
                            />
                          </div>
                          <span className="dashboard-project-sessions">
                            {project.session_count} session{project.session_count === 1 ? "" : "s"}
                          </span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </section>

              <section className="dashboard-section dashboard-column">
                <h3 className="dashboard-section-title">Spend by agent</h3>
                <div className="dashboard-agent-grid">
                  {KNOWN_AGENT_IDS.map((agentId) => {
                    const meta = agentMeta(agentId);
                    const usage = usageByAgent.get(agentId);
                    return (
                      <div className="dashboard-card dashboard-agent-card" key={agentId}>
                        <div className="dashboard-agent-header">
                          <span className="dashboard-agent-icon">{meta.icon}</span>
                          <span className="dashboard-agent-label">{meta.label}</span>
                        </div>
                        <div className="dashboard-agent-stats">
                          <div className="dashboard-agent-stat">
                            <span className="dashboard-agent-stat-value">
                              ${(usage?.total_cost_usd ?? 0).toFixed(2)}
                            </span>
                            <span className="dashboard-agent-stat-label">spend</span>
                          </div>
                          <div className="dashboard-agent-stat">
                            <span className="dashboard-agent-stat-value">
                              {usage?.session_count ?? 0}
                            </span>
                            <span className="dashboard-agent-stat-label">sessions</span>
                          </div>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </section>
            </div>
          </>
        ) : null}
      </section>

      {showComposer ? (
        <DispatchModal
          onClose={() => setShowComposer(false)}
          onDispatched={(created) => {
            const startedAt = created.run.started_at ?? created.run.created_at;
            onOpenAgentWork({
              taskId: created.task.id,
              runId: created.run.id,
              startedAt,
            });
          }}
        />
      ) : null}
    </div>
  );
}
