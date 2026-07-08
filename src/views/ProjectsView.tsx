import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { listProjects } from "../lib/tauri";
import { ProjectCard } from "./ProjectCard";
import { ProjectDetail } from "./ProjectDetail";
import "./ProjectsView.css";

export function ProjectsView() {
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);

  const { data, isLoading, isError } = useQuery({
    queryKey: ["projects"],
    queryFn: listProjects,
  });

  if (isLoading) {
    return <p className="projects-view-status">Loading projects…</p>;
  }

  if (isError) {
    return (
      <p className="projects-view-status">
        Couldn't load projects. Is the backend running?
      </p>
    );
  }

  if (!data || data.length === 0) {
    return (
      <p className="projects-view-status">
        No projects yet — start a Claude Code session in any repo and it will
        appear here.
      </p>
    );
  }

  // Auto-select the first project once data has loaded, rather than showing an extra
  // empty "select a project" state on first render — a reasonable default per the brief,
  // since there's always at least one project by this point.
  const selectedProject =
    data.find((project) => project.id === selectedProjectId) ?? data[0];

  return (
    <div className="projects-view">
      <div className="projects-view-list">
        {data.map((project) => (
          <ProjectCard
            key={project.id}
            project={project}
            selected={project.id === selectedProject.id}
            onClick={() => setSelectedProjectId(project.id)}
          />
        ))}
      </div>
      <div className="projects-view-detail">
        <ProjectDetail project={selectedProject} />
      </div>
    </div>
  );
}
