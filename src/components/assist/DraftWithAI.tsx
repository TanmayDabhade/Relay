import { useState } from "react";
import { draftTask } from "../../lib/tauri";
import type { TaskDraft } from "../../lib/types";
import "./Assist.css";

interface DraftWithAIProps {
  projectId: string;
  /** The rough note to expand — whatever the user has typed so far. */
  rough: string;
  currentTitle?: string;
  onDraft: (draft: TaskDraft) => void;
}

/** "Draft with AI": expands a rough note into a precise, self-contained agent prompt by
 * letting the local claude CLI read the project (read-only) first. */
export function DraftWithAI({ projectId, rough, currentTitle, onDraft }: DraftWithAIProps) {
  const [drafting, setDrafting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function draft() {
    setDrafting(true);
    setError(null);
    try {
      onDraft(await draftTask(projectId, rough, currentTitle));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setDrafting(false);
    }
  }

  return (
    <div className="assist-draft">
      <button
        type="button"
        className="assist-draft-button"
        onClick={draft}
        disabled={drafting || !projectId || !rough.trim()}
        title={
          rough.trim()
            ? "Read the project and rewrite this into a precise agent task"
            : "Write a rough note first"
        }
      >
        {drafting ? "Drafting… reading the repo" : "✦ Draft with AI"}
      </button>
      {error ? <span className="assist-error">{error}</span> : null}
    </div>
  );
}
