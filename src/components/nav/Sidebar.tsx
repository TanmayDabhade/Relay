import { useEffect, useRef, useState } from "react";
import { useActiveProject } from "../../hooks/useActiveProject";
import { ProjectDot } from "../ui/ProjectDot";
import "./Sidebar.css";

export type View =
  | "dashboard"
  | "board"
  | "projects"
  | "timeline"
  | "report"
  | "connections"
  | "settings"
  | "agent-viewer";

const NAV_ITEMS: { id: View; label: string; icon: string; disabled?: boolean }[] = [
  { id: "dashboard", label: "Home", icon: "▦" },
  { id: "board", label: "Board", icon: "▤" },
  { id: "projects", label: "Projects", icon: "⊞" },
  { id: "timeline", label: "Timeline", icon: "⋮" },
  { id: "report", label: "Reports", icon: "▥" },
  { id: "agent-viewer", label: "Agent work", icon: "▶" },
  { id: "connections", label: "Connections", icon: "⟳" },
  { id: "settings", label: "Settings", icon: "⚙" },
];

interface SidebarProps {
  active: View;
  onSelect: (view: View) => void;
  footer: string;
}

/** Picks the project the whole workspace follows (see `useActiveProject`). */
function ProjectSwitcher() {
  const { activeProject, activeProjectId, projects, setActiveProjectId } = useActiveProject();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function close(event: MouseEvent | KeyboardEvent) {
      if (event instanceof KeyboardEvent ? event.key === "Escape" : !rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open]);

  function choose(projectId: string | null) {
    setActiveProjectId(projectId);
    setOpen(false);
  }

  return (
    <div className="sidebar-switcher" ref={rootRef}>
      <button
        className="sidebar-switcher-button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        title={activeProject?.path}
      >
        {activeProject ? <ProjectDot projectId={activeProject.id} /> : <span className="sidebar-switcher-all" aria-hidden>◎</span>}
        <span className="sidebar-switcher-name">{activeProject?.name ?? "All projects"}</span>
        <span className="sidebar-switcher-caret" aria-hidden>▾</span>
      </button>
      {open ? (
        <ul className="sidebar-switcher-menu" role="listbox" aria-label="Active project">
          <li>
            <button
              role="option"
              aria-selected={activeProjectId === null}
              className={`sidebar-switcher-option${activeProjectId === null ? " selected" : ""}`}
              onClick={() => choose(null)}
            >
              <span className="sidebar-switcher-all" aria-hidden>◎</span>
              All projects
            </button>
          </li>
          {projects.map((project) => (
            <li key={project.id}>
              <button
                role="option"
                aria-selected={project.id === activeProjectId}
                className={`sidebar-switcher-option${project.id === activeProjectId ? " selected" : ""}`}
                onClick={() => choose(project.id)}
                title={project.path}
              >
                <ProjectDot projectId={project.id} />
                <span className="sidebar-switcher-name">{project.name}</span>
              </button>
            </li>
          ))}
          {projects.length === 0 ? (
            <li className="sidebar-switcher-empty">No projects yet</li>
          ) : null}
        </ul>
      ) : null}
    </div>
  );
}

export function Sidebar({ active, onSelect, footer }: SidebarProps) {
  return (
    <aside className="sidebar">
      <button className="sidebar-brand" onClick={() => onSelect("dashboard")}>
        Relay
      </button>
      <div className="sidebar-section-label">Workspace</div>
      <ProjectSwitcher />
      <nav>
        {NAV_ITEMS.map((item) => (
          <button
            key={item.id}
            className={`sidebar-nav-item${active === item.id ? " active" : ""}${item.disabled ? " disabled" : ""}`}
            aria-disabled={item.disabled}
            onClick={() => onSelect(item.id)}
          >
            <span className="sidebar-nav-icon">{item.icon}</span>
            {item.label}
          </button>
        ))}
      </nav>
      <div className="sidebar-footer">{footer}</div>
    </aside>
  );
}
