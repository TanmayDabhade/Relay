import { createContext, useContext } from "react";
import type { ProjectSummary } from "../lib/types";

export interface ActiveProjectValue {
  /** The project the workspace follows, or `null` for "All projects". */
  activeProjectId: string | null;
  activeProject: ProjectSummary | null;
  /** Every listed project, most recently active first (`listProjects` order). */
  projects: ProjectSummary[];
  projectsLoading: boolean;
  /** Pass `null` to show all projects. */
  setActiveProjectId: (projectId: string | null) => void;
}

export const ActiveProjectContext = createContext<ActiveProjectValue | null>(null);

export function useActiveProject(): ActiveProjectValue {
  const value = useContext(ActiveProjectContext);
  if (!value) throw new Error("useActiveProject must be used inside ActiveProjectProvider");
  return value;
}
