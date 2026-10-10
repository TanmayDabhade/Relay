import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "../components/ui/Button";
import { TextInput } from "../components/ui/TextInput";
import { useActiveProject } from "../hooks/useActiveProject";
import { addExistingProject, cloneProject, listProjects } from "../lib/tauri";
import type { AddedProject } from "../lib/types";
import { ProjectCard } from "./ProjectCard";
import { ProjectDetail } from "./ProjectDetail";
import "./ProjectsView.css";

interface ProjectsViewProps {
  /** Opens the Workspace board (the active project must already be set). */
  onOpenWorkspace: () => void;
}

type AddState =
  | { status: "idle" }
  | { status: "busy"; label: string }
  | { status: "error"; message: string }
  | { status: "added"; project: AddedProject };

/** Clone a GitHub repo, or register a checkout already on disk. Both require `gh` access,
 * since Relay only lists GitHub repositories (see `projects.rs` on the backend). */
function AddProjectPanel({ onAdded }: { onAdded: (project: AddedProject) => void }) {
  const queryClient = useQueryClient();
  const [link, setLink] = useState("");
  const [folder, setFolder] = useState("");
  const [state, setState] = useState<AddState>({ status: "idle" });
  const busy = state.status === "busy";

  async function run(label: string, action: () => Promise<AddedProject>) {
    setState({ status: "busy", label });
    try {
      const project = await action();
      await queryClient.invalidateQueries({ queryKey: ["projects"] });
      setState({ status: "added", project });
      setLink("");
      setFolder("");
      onAdded(project);
    } catch (reason) {
      setState({ status: "error", message: String(reason) });
    }
  }

  return (
    <div className="projects-add">
      <form
        className="projects-add-row"
        onSubmit={(event) => {
          event.preventDefault();
          if (link.trim()) void run("Cloning…", () => cloneProject(link.trim()));
        }}
      >
        <label className="projects-add-label" htmlFor="projects-add-link">
          Clone from GitHub
        </label>
        <TextInput
          id="projects-add-link"
          value={link}
          onChange={(event) => setLink(event.target.value)}
          placeholder="https://github.com/owner/name or owner/name"
          disabled={busy}
          spellCheck={false}
        />
        <Button type="submit" disabled={busy || !link.trim()}>
          Clone
        </Button>
      </form>
      <form
        className="projects-add-row"
        onSubmit={(event) => {
          event.preventDefault();
          if (folder.trim()) void run("Checking folder…", () => addExistingProject(folder.trim()));
        }}
      >
        <label className="projects-add-label" htmlFor="projects-add-folder">
          Add existing folder
        </label>
        <TextInput
          id="projects-add-folder"
          value={folder}
          onChange={(event) => setFolder(event.target.value)}
          placeholder="/Users/you/code/my-repo"
          disabled={busy}
          spellCheck={false}
        />
        <Button type="submit" variant="secondary" disabled={busy || !folder.trim()}>
          Add
        </Button>
      </form>
      <p className="projects-add-note" role="status">
        {state.status === "busy" && state.label}
        {state.status === "error" && <span className="projects-add-error">{state.message}</span>}
        {state.status === "added" &&
          `Added ${state.project.github_repo} at ${state.project.path}.`}
        {state.status === "idle" &&
          "Clones go into ~/RelayProjects. Relay lists GitHub repositories your gh can access."}
      </p>
    </div>
  );
}

export function ProjectsView({ onOpenWorkspace }: ProjectsViewProps) {
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const { setActiveProjectId } = useActiveProject();

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

  const selectedProject = data?.find((project) => project.id === selectedProjectId) ?? null;

  if (selectedProject) {
    return (
      <div className="projects-view-detail-page">
        <button className="projects-view-back" onClick={() => setSelectedProjectId(null)}>
          ← Back to projects
        </button>
        <ProjectDetail
          project={selectedProject}
          onOpenWorkspace={() => {
            setActiveProjectId(selectedProject.id);
            onOpenWorkspace();
          }}
        />
      </div>
    );
  }

  const empty = !data || data.length === 0;

  return (
    <div className="projects-view">
      <div className="projects-view-topbar">
        <h1 className="view-topbar-title">Projects</h1>
        {!empty ? (
          <Button variant="secondary" onClick={() => setShowAdd((value) => !value)}>
            {showAdd ? "Close" : "Add project"}
          </Button>
        ) : null}
      </div>

      {empty ? (
        <p className="projects-view-status">
          No projects yet. Relay lists folders that are GitHub repositories your <code>gh</code>{" "}
          can access. Ones you've run agents in show up automatically once checked, or add one
          below.
        </p>
      ) : null}

      {empty || showAdd ? (
        <AddProjectPanel onAdded={(project) => setActiveProjectId(project.id)} />
      ) : null}

      {!empty ? (
        <div className="projects-view-grid">
          {data.map((project) => (
            <ProjectCard
              key={project.id}
              project={project}
              onClick={() => {
                setActiveProjectId(project.id);
                setSelectedProjectId(project.id);
              }}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}
