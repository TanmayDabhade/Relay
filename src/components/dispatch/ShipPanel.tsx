import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getTaskWorkspace, openUrl, shipTask } from "../../lib/tauri";
import { SHIP_LABELS } from "../../lib/ship";
import { Button } from "../ui/Button";
import "../assist/Assist.css";

interface ShipPanelProps {
  taskId: string;
  /** A turn is in flight — the agent may still be editing, so shipping is disabled. */
  busy: boolean;
}

/** Branch + pull-request state for a task dispatched with "Open PR when done", with a manual
 * Ship button for when auto-shipping didn't run (loop hit its cap) or failed. Renders nothing
 * for tasks that ran in the project checkout itself. */
export function ShipPanel({ taskId, busy }: ShipPanelProps) {
  const queryClient = useQueryClient();
  const { data: workspace } = useQuery({
    queryKey: ["task-workspace", taskId],
    queryFn: () => getTaskWorkspace(taskId),
  });
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!workspace) return null;

  const shipping = workspace.ship_status === "shipping";
  const removed = workspace.removed_at != null;

  async function ship() {
    setStarting(true);
    setError(null);
    try {
      await shipTask(taskId);
      await queryClient.invalidateQueries({ queryKey: ["task-workspace", taskId] });
    } catch (reason) {
      setError(String(reason));
    } finally {
      setStarting(false);
    }
  }

  return (
    <div className="ship-panel">
      <span className={`ship-badge is-${workspace.ship_status}`}>
        {SHIP_LABELS[workspace.ship_status]}
      </span>
      <span className="ship-panel-branch" title={workspace.work_dir}>
        {workspace.branch} → {workspace.base_branch}
      </span>
      {workspace.pr_url ? (
        <button
          type="button"
          className="ship-panel-link"
          onClick={() => void openUrl(workspace.pr_url!)}
        >
          {workspace.pr_url.replace(/^https:\/\/github\.com\//, "")}
        </button>
      ) : null}
      <span className="ship-panel-spacer" />
      {!removed ? (
        <Button
          type="button"
          variant="secondary"
          onClick={ship}
          disabled={busy || shipping || starting}
          title={busy ? "Wait for the current turn to finish" : "Commit, push, and open or update the PR now"}
        >
          {shipping || starting
            ? "Shipping…"
            : workspace.pr_url
              ? "Push updates"
              : "Ship now"}
        </Button>
      ) : null}
      {workspace.ship_error ? <p className="ship-panel-error">{workspace.ship_error}</p> : null}
      {error ? <p className="ship-panel-error">{error}</p> : null}
    </div>
  );
}
