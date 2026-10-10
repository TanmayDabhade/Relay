import { ProjectBoard } from "../components/board/ProjectBoard";
import { ProjectDot } from "../components/ui/ProjectDot";
import { useActiveProject } from "../hooks/useActiveProject";
import "./WorkspaceView.css";

interface WorkspaceViewProps {
  onOpenProjects: () => void;
}

/** The kanban board for the project the sidebar switcher has selected. Switching project
 * swaps the board in place; `key` resets per-board UI state (open card, dispatch modal) so
 * nothing from one project's board carries over to another's. */
export function WorkspaceView({ onOpenProjects }: WorkspaceViewProps) {
  const { activeProject, projects, projectsLoading, setActiveProjectId } = useActiveProject();

  if (projectsLoading) {
    return <p className="workspace-view-status">Loading projects…</p>;
  }

  if (!activeProject) {
    return (
      <div className="workspace-view-empty">
        <h1 className="view-topbar-title">Board</h1>
        {projects.length === 0 ? (
          <p className="workspace-view-status">
            No projects yet. Clone or add a GitHub repository in{" "}
            <button className="workspace-view-link" onClick={onOpenProjects}>
              Projects
            </button>{" "}
            to get a board.
          </p>
        ) : (
          <>
            <p className="workspace-view-status">
              Each board belongs to one project. Pick one to open its board:
            </p>
            <div className="workspace-view-picker">
              {projects.map((project) => (
                <button
                  key={project.id}
                  className="workspace-view-pick"
                  onClick={() => setActiveProjectId(project.id)}
                >
                  <ProjectDot projectId={project.id} />
                  {project.name}
                </button>
              ))}
            </div>
          </>
        )}
      </div>
    );
  }

  return (
    <div className="workspace-view">
      <div className="workspace-view-header">
        <h1 className="workspace-view-name">
          <ProjectDot projectId={activeProject.id} />
          {activeProject.name}
        </h1>
        <div className="workspace-view-meta">
          {activeProject.github_repo ? <span>{activeProject.github_repo}</span> : null}
          <span className="workspace-view-path">{activeProject.path}</span>
        </div>
      </div>
      <ProjectBoard key={activeProject.id} projectId={activeProject.id} />
    </div>
  );
}
