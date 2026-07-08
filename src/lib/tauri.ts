import { invoke } from "@tauri-apps/api/core";
import type { ProjectSummary, Session, SessionDetail } from "./types";

export function listProjects(): Promise<ProjectSummary[]> {
  return invoke("list_projects");
}

export function listSessions(): Promise<Session[]> {
  return invoke("list_sessions");
}

export function getSessionDetail(sessionId: string): Promise<SessionDetail | null> {
  return invoke("get_session_detail", { sessionId });
}

export function openInEditor(path: string): Promise<void> {
  return invoke("open_in_editor", { path });
}

export function getProjectActivity(projectPath: string): Promise<number[]> {
  return invoke("project_activity", { projectPath });
}
