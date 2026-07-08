import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ActivityBars } from "../components/ui/ActivityBars";
import { StatTile } from "../components/ui/StatTile";
import { getProjectActivity, listSessions } from "../lib/tauri";
import type { ProjectSummary } from "../lib/types";
import { SessionDetailModal } from "./SessionDetailModal";
import { SessionRow } from "./SessionRow";
import "./ProjectDetail.css";

interface ProjectDetailProps {
  project: ProjectSummary;
}

/** All-zero 14-day placeholder shown while activity data hasn't loaded yet (or failed to,
 * e.g. outside a real Tauri context) — mirrors `ProjectCard`'s same fallback. */
const EMPTY_ACTIVITY = new Array(14).fill(0);

/**
 * Right-panel detail for the currently-selected project in `ProjectsView`'s master-detail
 * layout. A single scrollable column rather than literal tabs — this app's actual content
 * density (a handful of stats + a session list) reads fine always-visible, and avoids
 * hiding the session list behind an extra click for projects with only a couple of
 * sessions. Reuses `SessionRow` (shared with `SessionsView`) rather than re-implementing
 * row rendering here.
 */
export function ProjectDetail({ project }: ProjectDetailProps) {
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);

  const {
    data: sessions,
    isLoading,
    isError,
  } = useQuery({ queryKey: ["sessions"], queryFn: listSessions });

  const { data: activity } = useQuery({
    queryKey: ["project-activity", project.path],
    queryFn: () => getProjectActivity(project.path),
    retry: false,
  });

  const projectSessions = useMemo(
    () => sessions?.filter((session) => session.project_id === project.id) ?? [],
    [sessions, project.id],
  );

  return (
    <div className="project-detail">
      <div className="project-detail-overview">
        <div className="project-detail-header">
          <h1 className="project-detail-name">{project.name}</h1>
          <div className="project-detail-path">{project.path}</div>
        </div>

        <div className="project-detail-stats">
          <StatTile value={project.session_count} label="Sessions" />
          <StatTile value={`$${project.total_cost_usd.toFixed(2)}`} label="Total cost" />
        </div>

        <ActivityBars data={activity ?? EMPTY_ACTIVITY} />
      </div>

      <div className="project-detail-sessions">
        <h2 className="project-detail-sessions-title">Sessions</h2>

        {isLoading && <p className="project-detail-status">Loading sessions…</p>}

        {isError && (
          <p className="project-detail-status">Couldn't load sessions.</p>
        )}

        {!isLoading && !isError && projectSessions.length === 0 && (
          <p className="project-detail-status">No sessions yet for this project.</p>
        )}

        {!isLoading && !isError && projectSessions.length > 0 && (
          <div className="project-detail-session-list">
            {projectSessions.map((session) => (
              <SessionRow
                key={session.id}
                session={session}
                projectName={project.name}
                onClick={() => setSelectedSessionId(session.id)}
              />
            ))}
          </div>
        )}
      </div>

      <SessionDetailModal
        sessionId={selectedSessionId}
        onClose={() => setSelectedSessionId(null)}
      />
    </div>
  );
}
