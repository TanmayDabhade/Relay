import { invoke } from "@tauri-apps/api/core";
import type { ProjectSummary } from "./types";

export function listProjects(): Promise<ProjectSummary[]> {
  return invoke("list_projects");
}
