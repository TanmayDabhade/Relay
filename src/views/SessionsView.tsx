import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getPlanGating, listProjects, listSessions } from "../lib/tauri";
import { UpgradeBanner } from "../components/UpgradeBanner";
import { SessionDetailModal } from "./SessionDetailModal";
import { SessionRow } from "./SessionRow";
import type { ProjectSummary } from "../lib/types";
import "./SessionsView.css";

function projectNameFor(projects: ProjectSummary[] | undefined, projectId: string): string {
  return projects?.find((p) => p.id === projectId)?.name ?? projectId;
}

export function SessionsView() {
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);

  const {
    data: sessions,
    isLoading,
    isError,
  } = useQuery({ queryKey: ["sessions"], queryFn: listSessions });

  const { data: projects } = useQuery({ queryKey: ["projects"], queryFn: listProjects });

  const { data: gating } = useQuery({ queryKey: ["plan-gating"], queryFn: getPlanGating });
  const hiddenSessions = gating && !gating.is_paid ? gating.hidden_sessions : 0;

  return (
    <div className="sessions-view">
      <div className="view-topbar">
        <h1 className="view-topbar-title">Sessions</h1>
      </div>

      {hiddenSessions > 0 && (
        <UpgradeBanner
          message={`${hiddenSessions} ${hiddenSessions === 1 ? "session" : "sessions"} hidden on the free plan — upgrade to see ${hiddenSessions === 1 ? "it" : "them"}.`}
        />
      )}

      {isLoading && <p className="sessions-view-status">Loading sessions…</p>}

      {isError && (
        <p className="sessions-view-status">
          Couldn't load sessions. Is the backend running?
        </p>
      )}

      {!isLoading && !isError && (!sessions || sessions.length === 0) && (
        <p className="sessions-view-status">
          No sessions yet — start a Claude Code session in any repo and it will
          appear here.
        </p>
      )}

      {!isLoading && !isError && sessions && sessions.length > 0 && (
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
      )}

      <SessionDetailModal
        sessionId={selectedSessionId}
        onClose={() => setSelectedSessionId(null)}
      />
    </div>
  );
}
