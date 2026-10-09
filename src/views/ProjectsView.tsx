import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AddProjectModal } from "../components/projects/AddProjectModal";
import { Button } from "../components/ui/Button";
import { listProjects } from "../lib/tauri";
import { ProjectCard } from "./ProjectCard";
import { ProjectDetail } from "./ProjectDetail";
import "./ProjectsView.css";

export function ProjectsView() {
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [isAdding, setIsAdding] = useState(false);

  const { data, isLoading, isError } = useQuery({
    queryKey: ["projects"],
    queryFn: listProjects,
  });

  const addModal = isAdding ? (
    <AddProjectModal
      onClose={() => setIsAdding(false)}
      onAdded={(project) => {
        setIsAdding(false);
        setSelectedProjectId(project.id);
      }}
    />
  ) : null;

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
      <div className="projects-view-empty">
        <p className="projects-view-status">
          No projects yet. A project is a GitHub repo you can reach with <code>gh</code>: clone
          one from a link, or add a folder you've already checked out.
        </p>
        <Button onClick={() => setIsAdding(true)}>Add project</Button>
        {addModal}
      </div>
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

  return (
    <>
      <div className="projects-view-header">
        <Button onClick={() => setIsAdding(true)}>Add project</Button>
      </div>
      <div className="projects-view-grid">
        {data.map((project) => (
          <ProjectCard
            key={project.id}
            project={project}
            onClick={() => setSelectedProjectId(project.id)}
          />
        ))}
      </div>
      {addModal}
    </>
  );
}
