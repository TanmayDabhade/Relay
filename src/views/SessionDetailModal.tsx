import { useQuery } from "@tanstack/react-query";
import { Modal } from "../components/ui/Modal";
import { Pill } from "../components/ui/Pill";
import { getSessionDetail, openInEditor } from "../lib/tauri";
import "./SessionDetailModal.css";

interface SessionDetailModalProps {
  sessionId: string | null;
  onClose: () => void;
}

/** `tags` is stored as a JSON array string (e.g. `["bugfix","refactor"]`); fall back to
 * treating the raw string as a single tag if it doesn't parse, rather than hiding it. */
function parseTags(tags: string | null): string[] {
  if (!tags) return [];
  try {
    const parsed = JSON.parse(tags);
    if (Array.isArray(parsed)) {
      return parsed.filter((t): t is string => typeof t === "string");
    }
    return [tags];
  } catch {
    return [tags];
  }
}

function handleOpenInEditor(path: string) {
  openInEditor(path).catch((err) => {
    console.error(`Failed to open ${path} in editor:`, err);
  });
}

export function SessionDetailModal({ sessionId, onClose }: SessionDetailModalProps) {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["session-detail", sessionId],
    queryFn: () => getSessionDetail(sessionId!),
    enabled: sessionId !== null,
  });

  if (sessionId === null) {
    return null;
  }

  return (
    <Modal isOpen={sessionId !== null} onClose={onClose} title="Session detail">
      {isLoading && <p className="session-detail-status">Loading session…</p>}
      {isError && (
        <p className="session-detail-status">Couldn't load this session.</p>
      )}
      {!isLoading && !isError && !data && (
        <p className="session-detail-status">
          This session no longer exists.
        </p>
      )}
      {data && <SessionDetailContent detail={data} />}
    </Modal>
  );
}

function SessionDetailContent({
  detail,
}: {
  detail: NonNullable<Awaited<ReturnType<typeof getSessionDetail>>>;
}) {
  const { session, files_changed } = detail;
  const tags = parseTags(session.tags);

  return (
    <div className="session-detail">
      <div className="session-detail-stats">
        <div>
          <span className="session-detail-label">Status</span>
          <Pill variant="status" tone={session.status === "active" ? "green" : "gray"}>
            {session.status}
          </Pill>
        </div>
        <div>
          <span className="session-detail-label">Model</span>
          <span>{session.model ?? "unknown"}</span>
        </div>
        <div>
          <span className="session-detail-label">Duration</span>
          <span>
            {session.status === "ended" && session.duration_seconds !== null
              ? `${session.duration_seconds}s`
              : "—"}
          </span>
        </div>
        <div>
          <span className="session-detail-label">Cost</span>
          <span>${session.cost_usd.toFixed(2)}</span>
        </div>
        <div>
          <span className="session-detail-label">Prompt tokens</span>
          <span>{session.prompt_tokens}</span>
        </div>
        <div>
          <span className="session-detail-label">Completion tokens</span>
          <span>{session.completion_tokens}</span>
        </div>
        <div>
          <span className="session-detail-label">Cache read tokens</span>
          <span>{session.cache_read_tokens}</span>
        </div>
        <div>
          <span className="session-detail-label">Cache creation tokens</span>
          <span>{session.cache_creation_tokens}</span>
        </div>
        <div>
          <span className="session-detail-label">Lines added</span>
          <span>{session.lines_added}</span>
        </div>
        <div>
          <span className="session-detail-label">Lines removed</span>
          <span>{session.lines_removed}</span>
        </div>
      </div>

      {tags.length > 0 && (
        <div className="session-detail-tags">
          {tags.map((tag) => (
            <Pill key={tag} variant="tag">
              {tag}
            </Pill>
          ))}
        </div>
      )}

      <p className="session-detail-summary">
        {session.summary ?? "No summary yet"}
      </p>

      <div className="session-detail-files">
        <h3 className="session-detail-files-title">
          Files changed ({files_changed.length})
        </h3>
        {files_changed.length === 0 ? (
          <p className="session-detail-status">No file changes recorded.</p>
        ) : (
          <ul className="session-detail-file-list">
            {files_changed.map((file) => (
              <li key={file.id} className="session-detail-file-row">
                <div className="session-detail-file-info">
                  <span className="session-detail-file-path">{file.file_path}</span>
                  <span className="session-detail-file-meta">
                    {file.change_type} · +{file.lines_added} / -{file.lines_removed}
                  </span>
                </div>
                <button
                  className="session-detail-file-open"
                  onClick={() => handleOpenInEditor(file.file_path)}
                >
                  Open
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
