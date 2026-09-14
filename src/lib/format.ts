import type { DispatchStatus } from "./types";

/** A session's best available display name: Claude Code's own auto-generated title first
 * (matches "Session name" in `claude`'s `/status` and `--resume` picker), falling back to
 * Relay's own AI-generated one-line summary, then a fixed placeholder if neither exists yet. */
export function sessionDisplayName(
  title: string | null,
  summary: string | null,
): string {
  return title ?? summary ?? "Untitled session";
}

/** Formats a unix-seconds timestamp as a short relative time string, e.g. "5m ago". */
export function formatRelativeTime(unixSeconds: number): string {
  const diffMs = Date.now() - unixSeconds * 1000;
  const diffMin = Math.round(diffMs / 60000);
  if (diffMin < 1) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.round(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  return `${Math.round(diffHr / 24)}d ago`;
}

const ONGOING_DISPATCH_STATUSES = new Set<DispatchStatus>([
  "queued",
  "starting",
  "running",
  "awaiting_approval",
  "interrupting",
  "idle",
  "shutting_down",
]);

/** A Relay conversation remains ongoing while it can still advance or accept a follow-up. */
export function isOngoingDispatch(status: DispatchStatus): boolean {
  return ONGOING_DISPATCH_STATUSES.has(status);
}

/**
 * Spend for a session or group. `unpriced` means some usage is on a model Relay has no price
 * for (e.g. Codex's GPT models), so the dollar figure is a lower bound, or absent entirely.
 */
export function formatCost(costUsd: number, unpriced: boolean): string {
  if (unpriced && costUsd === 0) return "Not priced";
  return `$${costUsd.toFixed(2)}${unpriced ? "+" : ""}`;
}
