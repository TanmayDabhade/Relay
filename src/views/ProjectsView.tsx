import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getPlanGating, listProjects } from "../lib/tauri";
import { UpgradeBanner } from "../components/UpgradeBanner";
import { ProjectCard } from "./ProjectCard";
import { ProjectDetail } from "./ProjectDetail";
import "./ProjectsView.css";

export function ProjectsView() {
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);

  const { data, isLoading, isError } = useQuery({
    queryKey: ["projects"],
    queryFn: listProjects,
  });

  const { data: gating } = useQuery({ queryKey: ["plan-gating"], queryFn: getPlanGating });

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

  const selectedProject = data.find((project) => project.id === selectedProjectId) ?? null;

  if (selectedProject) {
    return (
      <div className="projects-view-detail-page">
        <button className="projects-view-back" onClick={() => setSelectedProjectId(null)}>
          ← Back to projects
        </button>
        <ProjectDetail project={selectedProject} />
      </div>
    );
  }

  const hiddenProjects = gating && !gating.is_paid ? gating.hidden_projects : 0;

  return (
    <>
      {hiddenProjects > 0 && (
        <UpgradeBanner
          message={`${hiddenProjects} ${hiddenProjects === 1 ? "project" : "projects"} hidden on the free plan — upgrade to see ${hiddenProjects === 1 ? "it" : "them"}.`}
        />
      )}
      <div className="projects-view-grid">
        {data.map((project) => (
          <ProjectCard
            key={project.id}
            project={project}
            onClick={() => setSelectedProjectId(project.id)}
          />
        ))}
      </div>
    </>
  );
}
