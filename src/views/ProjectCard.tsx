import { Pill } from "../components/ui/Pill";
import type { ProjectSummary } from "../lib/types";
import "./ProjectCard.css";

function formatRelativeTime(unixSeconds: number): string {
  const diffMs = Date.now() - unixSeconds * 1000;
  const diffMin = Math.round(diffMs / 60000);
  if (diffMin < 1) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.round(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  return `${Math.round(diffHr / 24)}d ago`;
}

interface ProjectCardProps {
  project: ProjectSummary;
}

export function ProjectCard({ project }: ProjectCardProps) {
  return (
    <div className="project-card">
      <div className="project-card-header">
        <span className="project-card-name">{project.name}</span>
        <Pill variant="agent" tone="accent">
          claude
        </Pill>
      </div>
      <div className="project-card-path">{project.path}</div>
      <div className="project-card-stats">
        <span>
          {project.session_count} session{project.session_count === 1 ? "" : "s"}
        </span>
        <span>${project.total_cost_usd.toFixed(2)} spent</span>
        <span>active {formatRelativeTime(project.last_active)}</span>
      </div>
    </div>
  );
}
