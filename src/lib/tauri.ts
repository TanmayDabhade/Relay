import { invoke } from "@tauri-apps/api/core";
import type {
  AgentConnection,
  BoardColumn,
  BoardData,
  Card,
  CreatedDispatch,
  DashboardStats,
  DispatchRun,
  DispatchApprovalDecision,
  DispatchConversation,
  DispatchTaskWithRun,
  FileDiff,
  CardShip,
  GitInsights,
  Plan,
  ProjectSummary,
  ReportData,
  Session,
  SessionDetail,
  TaskDraft,
  TaskWorkspace,
} from "./types";

export function listProjects(): Promise<ProjectSummary[]> {
  return invoke("list_projects");
}

export function listSessions(): Promise<Session[]> {
  return invoke("list_sessions");
}

export function getSessionDetail(sessionId: string): Promise<SessionDetail | null> {
  return invoke("get_session_detail", { sessionId });
}

export function deleteSession(sessionId: string): Promise<void> {
  return invoke("delete_session", { sessionId });
}

export function openInEditor(path: string): Promise<void> {
  return invoke("open_in_editor", { path });
}

export function getProjectActivity(projectPath: string): Promise<number[]> {
  return invoke("project_activity", { projectPath });
}

export function getProjectGitInsights(projectPath: string): Promise<GitInsights> {
  return invoke("project_git_insights", { projectPath });
}

export function getDashboardStats(): Promise<DashboardStats> {
  return invoke("dashboard_stats");
}

export function generateReport(rangeDays: number): Promise<ReportData> {
  return invoke("generate_report", { rangeDays });
}

/** Writes the report as Markdown to the user's Downloads folder and resolves to the
 * absolute path it was saved at, for a "Reveal in Finder" follow-up action. */
export function exportReport(rangeDays: number): Promise<string> {
  return invoke("export_report", { rangeDays });
}

export function revealInFinder(path: string): Promise<void> {
  return invoke("reveal_in_finder", { path });
}

export function openUrl(url: string): Promise<void> {
  return invoke("open_url", { url });
}

/** Writes the session's transcript as Markdown to the user's Downloads folder and resolves
 * to the absolute path it was saved at, for a "Reveal in Finder" follow-up action. */
export function exportTranscript(sessionId: string): Promise<string> {
  return invoke("export_transcript", { sessionId });
}

/** Returns the session's transcript as Markdown without writing an export file. */
export function getTranscriptMarkdown(sessionId: string): Promise<string> {
  return invoke("get_transcript_markdown", { sessionId });
}

export function getFileDiffForSessionFile(sessionId: string, filePath: string): Promise<FileDiff | null> {
  return invoke("get_file_diff_for_session_file", { sessionId, filePath });
}

export function getBoard(projectId: string): Promise<BoardData> {
  return invoke("get_board", { projectId });
}

export function createCard(
  boardId: string,
  columnId: string,
  title: string,
  description?: string,
): Promise<Card> {
  return invoke("create_card", { boardId, columnId, title, description: description ?? null });
}

export function moveCard(cardId: string, columnId: string, position: number): Promise<void> {
  return invoke("move_card", { cardId, columnId, position });
}

export function updateCard(cardId: string, title: string, description?: string): Promise<void> {
  return invoke("update_card", { cardId, title, description: description ?? null });
}

export function deleteCard(cardId: string): Promise<void> {
  return invoke("delete_card", { cardId });
}

export function linkSessionToCard(cardId: string, sessionId: string): Promise<void> {
  return invoke("link_session_to_card", { cardId, sessionId });
}

export function createColumn(boardId: string, name: string): Promise<BoardColumn> {
  return invoke("create_column", { boardId, name });
}

export function renameColumn(columnId: string, name: string): Promise<void> {
  return invoke("rename_column", { columnId, name });
}

/** Attaches the card's title/description to a live `claude` terminal session for its
 * project (or opens a new one) — see `terminal::attach_or_launch` on the backend for the
 * full behavior. Resolves to a short outcome string (or a "skipped: ..." reason if the card
 * wasn't eligible), never rejects for an ineligible card — only for an actual failure to
 * reach/drive Terminal.app. */
export function launchOrAttachSession(cardId: string): Promise<string> {
  return invoke("launch_or_attach_session", { cardId });
}

export function listAgentConnections(): Promise<AgentConnection[]> {
  return invoke("list_agent_connections");
}

export function saveAgentConnection(connection: {
  agent: string;
  enabled: boolean;
  executable: string;
  models: string[];
  defaultModel: string;
}): Promise<void> {
  return invoke("save_agent_connection", connection);
}

export function dispatchTask(request: {
  projectId: string;
  cardId?: string | null;
  title: string;
  prompt: string;
  agent: string;
  model: string;
  /** Loop until the agent reports done, up to this many continuations; omit for one turn. */
  loopMaxIterations?: number | null;
  /** Run in an isolated git worktree and commit, push, and open a PR when the task finishes. */
  openPr?: boolean;
}): Promise<CreatedDispatch> {
  return invoke("dispatch_task", {
    ...request,
    cardId: request.cardId ?? null,
    loopMaxIterations: request.loopMaxIterations ?? null,
    openPr: request.openPr ?? false,
  });
}

export function retryDispatchTask(
  taskId: string,
  agent: string,
  model: string,
): Promise<CreatedDispatch> {
  return invoke("retry_dispatch_task", { taskId, agent, model });
}

export function listDispatchTasks(dayStart: number, dayEnd: number): Promise<DispatchTaskWithRun[]> {
  return invoke("list_dispatch_tasks", { dayStart, dayEnd });
}

export function listDispatchRuns(taskId: string): Promise<DispatchRun[]> {
  return invoke("list_dispatch_runs", { taskId });
}

export function getDispatchConversation(runId: string): Promise<DispatchConversation | null> {
  return invoke("get_dispatch_conversation", { runId });
}

export function sendDispatchPrompt(runId: string, prompt: string): Promise<void> {
  return invoke("send_dispatch_prompt", { runId, prompt });
}

export function stopDispatchLoop(runId: string): Promise<void> {
  return invoke("stop_dispatch_loop", { runId });
}

export function interruptDispatchTurn(runId: string): Promise<void> {
  return invoke("interrupt_dispatch_turn", { runId });
}

export function resolveDispatchApproval(
  runId: string,
  eventId: number,
  decision: DispatchApprovalDecision,
): Promise<void> {
  return invoke("resolve_dispatch_approval", { runId, eventId, decision });
}

export function shutdownDispatchConversation(runId: string): Promise<void> {
  return invoke("shutdown_dispatch_conversation", { runId });
}

export function getTaskWorkspace(taskId: string): Promise<TaskWorkspace | null> {
  return invoke("get_task_workspace", { taskId });
}

export function listCardShips(projectId: string): Promise<CardShip[]> {
  return invoke("list_card_ships", { projectId });
}

/** Commits, pushes, and opens (or updates) the task's PR now. Resolves once the ship has
 * started; the outcome arrives as a `data-changed` event and a notice in the conversation. */
export function shipTask(taskId: string): Promise<void> {
  return invoke("ship_task", { taskId });
}

/** Turns a rough note into a precise agent task by running the local claude CLI read-only in
 * the project. Takes tens of seconds. */
export function draftTask(
  projectId: string,
  rough: string,
  currentTitle?: string,
): Promise<TaskDraft> {
  return invoke("draft_task", { projectId, rough, currentTitle: currentTitle ?? null });
}

/** Breaks a goal into ordered, dispatchable tasks after reading the project. Takes a minute
 * or more on larger repos. */
export function planTasks(projectId: string, goal: string): Promise<Plan> {
  return invoke("plan_tasks", { projectId, goal });
}

/** Adds approved plan tasks to the project's Todo column, in order. */
export function createPlannedCards(
  projectId: string,
  tasks: { title: string; prompt: string }[],
): Promise<Card[]> {
  return invoke("create_planned_cards", { projectId, tasks });
}
