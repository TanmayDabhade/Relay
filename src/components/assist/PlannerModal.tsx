import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { createPlannedCards, planTasks } from "../../lib/tauri";
import type { PlannedTask } from "../../lib/types";
import { Button } from "../ui/Button";
import { Modal } from "../ui/Modal";
import "../dispatch/DispatchModal.css";
import "./Assist.css";

interface PlannerModalProps {
  projectId: string;
  projectName?: string;
  onClose: () => void;
}

interface DraftRow extends PlannedTask {
  selected: boolean;
}

/** Planner assistant: describe a goal, let the local claude CLI read the project and propose
 * an ordered set of agent tasks, edit or drop any of them, then add them to Todo. Nothing is
 * dispatched from here — the cards are dispatched one at a time from the board as usual. */
export function PlannerModal({ projectId, projectName, onClose }: PlannerModalProps) {
  const queryClient = useQueryClient();
  const [goal, setGoal] = useState("");
  const [summary, setSummary] = useState<string | null>(null);
  const [rows, setRows] = useState<DraftRow[]>([]);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [planning, setPlanning] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selected = rows.filter((row) => row.selected && row.title.trim());

  async function plan(event: React.FormEvent) {
    event.preventDefault();
    setPlanning(true);
    setError(null);
    try {
      const result = await planTasks(projectId, goal);
      setSummary(result.summary);
      setRows(result.tasks.map((task) => ({ ...task, selected: true })));
      setExpanded(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setPlanning(false);
    }
  }

  async function addCards() {
    setSaving(true);
    setError(null);
    try {
      await createPlannedCards(
        projectId,
        selected.map(({ title, prompt }) => ({ title, prompt })),
      );
      await queryClient.invalidateQueries({ queryKey: ["board", projectId] });
      onClose();
    } catch (reason) {
      setError(String(reason));
      setSaving(false);
    }
  }

  function update(index: number, patch: Partial<DraftRow>) {
    setRows((current) => current.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  }

  return (
    <Modal isOpen onClose={onClose} title="Plan with AI" wide>
      <div className="planner">
        <form className="planner-goal" onSubmit={plan}>
          <label className="dispatch-field">
            <span>What do you want to get done{projectName ? ` in ${projectName}` : ""}?</span>
            <textarea
              value={goal}
              onChange={(event) => setGoal(event.target.value)}
              rows={3}
              autoFocus
              placeholder="e.g. Let users export their board as CSV, with tests"
              onKeyDown={(event) => {
                if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                  event.currentTarget.form?.requestSubmit();
                }
              }}
            />
          </label>
          <div className="planner-goal-actions">
            <small>
              Runs your local <code>claude</code> CLI read-only in the project. It can read code
              but can't change anything.
            </small>
            <Button type="submit" variant={rows.length ? "secondary" : "primary"} disabled={planning || !goal.trim()}>
              {planning ? "Reading the repo and planning…" : rows.length ? "Re-plan" : "Plan"}
            </Button>
          </div>
        </form>

        {error ? <p className="dispatch-form-error">{error}</p> : null}

        {summary ? <p className="planner-summary">{summary}</p> : null}

        {rows.length ? (
          <ol className="planner-tasks">
            {rows.map((row, index) => (
              <li key={index} className={`planner-task${row.selected ? "" : " is-dropped"}`}>
                <div className="planner-task-head">
                  <input
                    type="checkbox"
                    checked={row.selected}
                    onChange={(event) => update(index, { selected: event.target.checked })}
                    aria-label={`Include task ${index + 1}`}
                  />
                  <span className="planner-task-index">{index + 1}</span>
                  <input
                    className="planner-task-title"
                    value={row.title}
                    onChange={(event) => update(index, { title: event.target.value })}
                  />
                  <button
                    type="button"
                    className="planner-task-toggle"
                    onClick={() => setExpanded(expanded === index ? null : index)}
                  >
                    {expanded === index ? "Hide prompt" : "Edit prompt"}
                  </button>
                </div>
                {row.rationale ? <p className="planner-task-rationale">{row.rationale}</p> : null}
                {expanded === index ? (
                  <textarea
                    className="planner-task-prompt"
                    value={row.prompt}
                    onChange={(event) => update(index, { prompt: event.target.value })}
                    rows={10}
                  />
                ) : null}
              </li>
            ))}
          </ol>
        ) : null}

        <div className="dispatch-form-actions">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="button" onClick={addCards} disabled={saving || planning || selected.length === 0}>
            {saving
              ? "Adding…"
              : `Add ${selected.length || ""} card${selected.length === 1 ? "" : "s"} to Todo`}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
