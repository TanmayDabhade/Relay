import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { addExistingProject, cloneProject } from "../../lib/tauri";
import type { ProjectSummary } from "../../lib/types";
import { Button } from "../ui/Button";
import { Modal } from "../ui/Modal";
import "../dispatch/DispatchModal.css";
import "./AddProjectModal.css";

type Source = "github" | "local";

interface AddProjectModalProps {
  onClose: () => void;
  /** Called with the new project once it's registered and the projects list is refreshed. */
  onAdded: (project: ProjectSummary) => void;
}

/** Adds a project either by cloning a GitHub repo through `gh` or by registering a folder
 * that's already checked out. The path is typed in, not picked: the Tauri dialog plugin isn't
 * installed. */
export function AddProjectModal({ onClose, onAdded }: AddProjectModalProps) {
  const queryClient = useQueryClient();
  const [source, setSource] = useState<Source>("github");
  const [repo, setRepo] = useState("");
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const value = source === "github" ? repo.trim() : path.trim();

  // A clone can't be cancelled, so the modal stays open until it settles.
  function close() {
    if (!busy) onClose();
  }

  function switchSource(next: Source) {
    setSource(next);
    setError(null);
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const project =
        source === "github" ? await cloneProject(value) : await addExistingProject(value);
      await queryClient.invalidateQueries({ queryKey: ["projects"] });
      onAdded(project);
    } catch (reason) {
      setError(String(reason));
      setBusy(false);
    }
  }

  return (
    <Modal isOpen onClose={close} title="Add project">
      <form className="dispatch-form" onSubmit={submit}>
        <div className="add-project-tabs" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={source === "github"}
            className={`add-project-tab${source === "github" ? " is-active" : ""}`}
            onClick={() => switchSource("github")}
            disabled={busy}
          >
            Clone from GitHub
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={source === "local"}
            className={`add-project-tab${source === "local" ? " is-active" : ""}`}
            onClick={() => switchSource("local")}
            disabled={busy}
          >
            Existing folder
          </button>
        </div>

        {source === "github" ? (
          <label className="dispatch-field">
            <span>GitHub repo</span>
            <input
              value={repo}
              onChange={(event) => setRepo(event.target.value)}
              placeholder="https://github.com/owner/name or owner/name"
              autoFocus
              disabled={busy}
              spellCheck={false}
            />
            <small>Cloned with your local <code>gh</code>, so private repos you can access work too.</small>
          </label>
        ) : (
          <label className="dispatch-field">
            <span>Folder path</span>
            <input
              value={path}
              onChange={(event) => setPath(event.target.value)}
              placeholder="~/code/my-repo"
              autoFocus
              disabled={busy}
              spellCheck={false}
            />
            <small>A git checkout whose GitHub remote <code>gh</code> can access.</small>
          </label>
        )}

        {error ? <p className="dispatch-form-error">{error}</p> : null}

        <div className="dispatch-form-actions">
          <Button type="button" variant="secondary" onClick={close} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" disabled={busy || !value}>
            {source === "github"
              ? busy
                ? "Cloning…"
                : "Clone and add"
              : busy
                ? "Adding…"
                : "Add project"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
