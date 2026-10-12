import type { ShipStatus } from "./types";

export const SHIP_LABELS: Record<ShipStatus, string> = {
  pending: "PR pending",
  shipping: "Shipping…",
  shipped: "PR open",
  no_changes: "No changes",
  failed: "Ship failed",
};

/** "PR #12" for a GitHub pull-request URL, or plain "PR" for anything unrecognized. */
export function prLabel(url: string): string {
  const number = url.match(/\/pull\/(\d+)/)?.[1];
  return number ? `PR #${number}` : "PR";
}
