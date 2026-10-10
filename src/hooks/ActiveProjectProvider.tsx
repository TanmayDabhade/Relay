import { useCallback, useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { listProjects } from "../lib/tauri";
import { ActiveProjectContext, type ActiveProjectValue } from "./useActiveProject";

const STORAGE_KEY = "relay.activeProject";
/** Stored instead of a project id when the user picked "All projects". */
const ALL_PROJECTS = "all";

function readStored(): string | null {
  try {
    return window.localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function writeStored(value: string | null) {
  try {
    if (value === null) window.localStorage.removeItem(STORAGE_KEY);
    else window.localStorage.setItem(STORAGE_KEY, value);
  } catch {
    // Storage can be unavailable; the choice then lasts for this window only.
  }
}

/**
 * Holds which project the workspace (Board, Home, Timeline, Agent work, Reports) is scoped
 * to. The choice is persisted, and falls back to the most recently active project when the
 * stored one is no longer listed (deleted, or no longer verified on GitHub) or nothing was
 * stored yet. "All projects" is a deliberate choice and is kept as-is.
 */
export function ActiveProjectProvider({ children }: { children: React.ReactNode }) {
  const [stored, setStored] = useState<string | null>(readStored);
  const { data: projects = [], isLoading: projectsLoading } = useQuery({
    queryKey: ["projects"],
    queryFn: listProjects,
  });

  const activeProjectId = useMemo(() => {
    if (stored === ALL_PROJECTS) return null;
    // Until the list arrives, trust the stored id so views don't flash unscoped data.
    if (projectsLoading) return stored;
    if (stored && projects.some((project) => project.id === stored)) return stored;
    return projects[0]?.id ?? null;
  }, [stored, projects, projectsLoading]);

  // Nothing to fall back to: forget a stale id so a later first project becomes active.
  useEffect(() => {
    if (!projectsLoading && projects.length === 0 && stored !== null && stored !== ALL_PROJECTS) {
      setStored(null);
      writeStored(null);
    }
  }, [projectsLoading, projects.length, stored]);

  const setActiveProjectId = useCallback((projectId: string | null) => {
    const value = projectId ?? ALL_PROJECTS;
    setStored(value);
    writeStored(value);
  }, []);

  const value = useMemo<ActiveProjectValue>(
    () => ({
      activeProjectId,
      activeProject: projects.find((project) => project.id === activeProjectId) ?? null,
      projects,
      projectsLoading,
      setActiveProjectId,
    }),
    [activeProjectId, projects, projectsLoading, setActiveProjectId],
  );

  return <ActiveProjectContext.Provider value={value}>{children}</ActiveProjectContext.Provider>;
}
