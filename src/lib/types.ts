export interface ProjectSummary {
  id: string;
  name: string;
  path: string;
  lang: string | null;
  stack: string | null;
  created_at: number;
  last_active: number;
  session_count: number;
  total_cost_usd: number;
  /** Every distinct agent with at least one session in this project. */
  agents: string[];
}

export interface FileChanged {
  id: number;
  session_id: string;
  file_path: string;
  change_type: "write" | "edit" | "multi_edit" | "notebook_edit";
  lines_added: number;
  lines_removed: number;
  occurred_at: number;
}

export interface Session {
  id: string;
  project_id: string;
  agent: string;
  model: string | null;
  started_at: number | null;
  ended_at: number | null;
  last_activity_at: number;
  status: "active" | "ended";
  duration_seconds: number | null;
  summary: string | null;
  /** Claude Code's own auto-generated session title — the same text shown as "Session name"
   * in `claude`'s `/status` and the `--resume` picker. Null until Claude has generated one. */
  title: string | null;
  prompt_tokens: number;
  completion_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  /** 1-hour-TTL share of `cache_creation_tokens` (billed at 2x input, vs 1.25x for 5-minute). */
  cache_creation_1h_tokens: number;
  cost_usd: number;
  /** Some usage is on a model with no known price, so `cost_usd` covers only the priced part. */
  cost_unpriced: boolean;
  lines_added: number;
  lines_removed: number;
  tags: string | null;
  raw_log_path: string;
}

export interface SessionDetail {
  session: Session;
  files_changed: FileChanged[];
}

export interface DailyActivity {
  /** `YYYY-MM-DD`. */
  date: string;
  count: number;
}

export interface CommitInfo {
  hash: string;
  message: string;
  author: string;
  timestamp: number;
}

export interface GitInsights {
  /** Oldest first, one entry per day, 365 days long (today inclusive) — commit counts, not
   * session/usage activity. */
  commit_heatmap: DailyActivity[];
  /** Newest first. Empty if the project isn't a git repo (or `git` isn't on `PATH`). */
  recent_commits: CommitInfo[];
}

export interface AgentUsage {
  agent: string;
  session_count: number;
  total_cost_usd: number;
  /** Sessions with usage on a model that has no known price (not included in the total). */
  unpriced_session_count: number;
}

export interface DiffLine {
  tag: "insert" | "delete" | "equal";
  content: string;
}

export interface FileDiff {
  lines: DiffLine[];
  truncated: boolean;
  /** When the most recent edit folded into this diff occurred. */
  occurred_at: number;
  /** How many separate tool-call edits (across the session) were folded into this diff. */
  edit_count: number;
}

export interface ActiveSessionSummary {
  session_id: string;
  session_title: string | null;
  session_summary: string | null;
  project_id: string;
  project_name: string;
}

export interface DashboardStats {
  total_cost_usd: number;
  total_sessions: number;
  /** Count of every project Relay knows about, independent of `top_projects`'s cap. */
  total_projects: number;
  /** Oldest first, one entry per day, 365 days long (today inclusive). */
  daily_activity: DailyActivity[];
  /** Highest-spend projects first, capped for display. */
  top_projects: ProjectSummary[];
  agent_usage: AgentUsage[];
  /** The most-recently-active session, if any is currently `status: "active"`. */
  active_session: ActiveSessionSummary | null;
}

export interface ReportTotals {
  total_cost_usd: number;
  session_count: number;
  prompt_tokens: number;
  completion_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
}

export interface ReportProjectRow {
  project_id: string;
  project_name: string;
  session_count: number;
  total_cost_usd: number;
}

export interface ReportTagRow {
  tag: string;
  session_count: number;
  total_cost_usd: number;
}

export interface ReportData {
  range_days: number;
  since_epoch: number;
  totals: ReportTotals;
  by_project: ReportProjectRow[];
  /** A session with multiple tags contributes to each — spend here can sum to more than
   * `totals.total_cost_usd`, see `report_by_tag` on the backend. */
  by_tag: ReportTagRow[];
  by_agent: AgentUsage[];
}

export interface Board {
  id: string;
  project_id: string;
  created_at: number;
}

export type ColumnRole = "todo" | "in_progress" | "review" | "done";

export interface BoardColumn {
  id: string;
  board_id: string;
  name: string;
  /** Only the four seeded columns carry a role; user-added columns are always null. */
  role: ColumnRole | null;
  position: number;
  created_at: number;
}

export interface Card {
  id: string;
  board_id: string;
  column_id: string;
  /** Set once a session is linked; auto-sync only ever moves cards that have this. */
  session_id: string | null;
  title: string;
  description: string | null;
  position: number;
  created_at: number;
  updated_at: number;
}

export interface BoardData {
  board: Board;
  columns: BoardColumn[];
  cards: Card[];
}

export type BuiltInAgent = "claude" | "codex" | "gemini" | "cursor";

export interface AgentConnection {
  agent: BuiltInAgent;
  enabled: boolean;
  executable: string;
  models: string[];
  default_model: string;
  updated_at: number;
  installed: boolean;
  resolved_executable: string | null;
}

export type DispatchStatus =
  | "queued"
  | "starting"
  | "running"
  | "awaiting_approval"
  | "interrupting"
  | "idle"
  | "shutting_down"
  | "shut_down"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted";

export interface DispatchTask {
  id: string;
  project_id: string;
  card_id: string | null;
  title: string;
  prompt: string;
  status: DispatchStatus;
  created_at: number;
  updated_at: number;
  completed_at: number | null;
}

export interface DispatchRun {
  id: string;
  task_id: string;
  attempt: number;
  agent: BuiltInAgent;
  model: string;
  status: DispatchStatus;
  started_at: number | null;
  ended_at: number | null;
  exit_code: number | null;
  error: string | null;
  session_id: string | null;
  provider_session_id: string | null;
  shutdown_at: number | null;
  created_at: number;
  /** Null when the run isn't looping; otherwise the cap on automatic continuations. */
  loop_max_iterations: number | null;
  /** Automatic continuation turns Relay has sent so far. */
  loop_iterations: number;
}

export interface CreatedDispatch {
  task: DispatchTask;
  run: DispatchRun;
}

/** DispatchTask fields are flattened by the backend for convenient workday list rendering. */
export interface DispatchTaskWithRun extends DispatchTask {
  project_name: string;
  project_path: string;
  run: DispatchRun;
}

export interface DispatchRunEvent {
  id: number;
  run_id: string;
  sequence: number;
  stream: "output" | "input";
  content: string;
  created_at: number;
}

export interface DispatchTurn {
  id: string;
  run_id: string;
  sequence: number;
  prompt: string;
  status: string;
  started_at: number;
  ended_at: number | null;
  error: string | null;
}

export type DispatchEventKind =
  | "user_message"
  | "assistant_message"
  | "tool_call"
  | "tool_result"
  | "approval_request"
  | "approval_decision"
  | "status"
  | "error"
  | "legacy_output";

export interface DispatchEvent {
  id: number;
  run_id: string;
  turn_id: string | null;
  sequence: number;
  kind: DispatchEventKind;
  role: "user" | "assistant" | null;
  content: string;
  payload: string | null;
  provider_event_id: string | null;
  state: string | null;
  created_at: number;
}

export interface DispatchConversation {
  run: DispatchRun;
  turns: DispatchTurn[];
  events: DispatchEvent[];
  legacy: boolean;
}

export type DispatchApprovalDecision =
  | "allowed_once"
  | "allowed_for_session"
  | "denied";

export type ShipStatus = "pending" | "shipping" | "shipped" | "no_changes" | "failed";

/** A PR-bound task's isolated git worktree and where its pull request stands. */
export interface TaskWorkspace {
  task_id: string;
  project_path: string;
  repo_root: string;
  worktree_path: string;
  /** Where the agent runs: the worktree plus the project's subdirectory within its repo. */
  work_dir: string;
  branch: string;
  base_branch: string;
  auto_ship: boolean;
  ship_status: ShipStatus;
  ship_error: string | null;
  pr_url: string | null;
  last_shipped_at: number | null;
  /** Set once the worktree folder is cleaned up (the branch and PR remain). */
  removed_at: number | null;
  created_at: number;
}

export interface CardShip {
  card_id: string;
  task_id: string;
  branch: string;
  ship_status: ShipStatus;
  pr_url: string | null;
}

export interface TaskDraft {
  title: string;
  prompt: string;
}

export interface PlannedTask {
  title: string;
  prompt: string;
  rationale: string;
}

export interface Plan {
  summary: string;
  tasks: PlannedTask[];
}
