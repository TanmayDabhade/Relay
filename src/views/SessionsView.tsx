import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { listProjects, listSessions } from "../lib/tauri";
import { formatRelativeTime } from "../lib/format";
import { Pill } from "../components/ui/Pill";
import { SessionDetailModal } from "./SessionDetailModal";
import type { ProjectSummary, Session } from "../lib/types";
import "./SessionsView.css";

function projectNameFor(projects: ProjectSummary[] | undefined, projectId: string): string {
  return projects?.find((p) => p.id === projectId)?.name ?? projectId;
}

interface SessionRowProps {
  session: Session;
  projectName: string;
  onClick: () => void;
}

function SessionRow({ session, projectName, onClick }: SessionRowProps) {
  return (
    <button className="session-row" onClick={onClick}>
      <div className="session-row-main">
        <div className="session-row-top">
          <span className="session-row-project">{projectName}</span>
          <Pill variant="status" tone={session.status === "active" ? "green" : "gray"}>
            {session.status}
          </Pill>
          <span className="session-row-model">{session.model ?? "unknown model"}</span>
        </div>
        <div className="session-row-summary">{session.summary ?? "No summary yet"}</div>
      </div>
      <div className="session-row-stats">
        <span>{formatRelativeTime(session.last_activity_at)}</span>
        <span>${session.cost_usd.toFixed(2)}</span>
        <span>
          {session.prompt_tokens + session.completion_tokens} tokens
        </span>
      </div>
    </button>
  );
}

export function SessionsView() {
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);

  const {
    data: sessions,
    isLoading,
    isError,
  } = useQuery({ queryKey: ["sessions"], queryFn: listSessions });

  const { data: projects } = useQuery({ queryKey: ["projects"], queryFn: listProjects });

  if (isLoading) {
    return <p className="sessions-view-status">Loading sessions…</p>;
  }

  if (isError) {
    return (
      <p className="sessions-view-status">
        Couldn't load sessions. Is the backend running?
      </p>
    );
  }

  if (!sessions || sessions.length === 0) {
    return (
      <p className="sessions-view-status">
        No sessions yet — start a Claude Code session in any repo and it will
        appear here.
      </p>
    );
  }

  return (
    <div className="sessions-view">
      <div className="sessions-view-list">
        {sessions.map((session) => (
          <SessionRow
            key={session.id}
            session={session}
            projectName={projectNameFor(projects, session.project_id)}
            onClick={() => setSelectedSessionId(session.id)}
          />
        ))}
      </div>
      <SessionDetailModal
        sessionId={selectedSessionId}
        onClose={() => setSelectedSessionId(null)}
      />
    </div>
  );
}
