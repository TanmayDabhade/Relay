import { Pill } from "../components/ui/Pill";
import { ProjectDot } from "../components/ui/ProjectDot";
import { formatCost, formatRelativeTime, sessionDisplayName } from "../lib/format";
import type { Session } from "../lib/types";
import "./SessionRow.css";

interface SessionRowProps {
  session: Session;
  projectName: string;
  onClick: () => void;
}

/**
 * Single-session summary row: project + status + model on top, summary below, stats
 * (relative time / cost / tokens) on the right. Used by `ProjectDetail`'s
 * Sessions tab (sessions scoped to one project); the app-wide list lives in `TimelineView`.
 */
export function SessionRow({ session, projectName, onClick }: SessionRowProps) {
  return (
    <button className="session-row" onClick={onClick}>
      <div className="session-row-main">
        <div className="session-row-top">
          <ProjectDot projectId={session.project_id} />
          <span className="session-row-project">{projectName}</span>
          <Pill variant="status" tone={session.status === "active" ? "green" : "gray"}>
            {session.status}
          </Pill>
          <span className="session-row-model">{session.model ?? "unknown model"}</span>
        </div>
        <div className="session-row-summary">
          {sessionDisplayName(session.title, session.summary)}
        </div>
      </div>
      <div className="session-row-stats">
        <span>{formatRelativeTime(session.last_activity_at)}</span>
        <span>{formatCost(session.cost_usd, session.cost_unpriced)}</span>
        <span>
          {session.prompt_tokens + session.completion_tokens} tokens
        </span>
      </div>
    </button>
  );
}
