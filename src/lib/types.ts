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
  prompt_tokens: number;
  completion_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  cost_usd: number;
  lines_added: number;
  lines_removed: number;
  tags: string | null;
  raw_log_path: string;
}

export interface SessionDetail {
  session: Session;
  files_changed: FileChanged[];
}
