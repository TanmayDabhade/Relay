import assert from "node:assert/strict";
import test from "node:test";

import * as format from "../src/lib/format.ts";

test("ongoing Relay work includes idle conversations but excludes terminal runs", () => {
  const candidate = (format as Record<string, unknown>).isOngoingDispatch;

  assert.equal(typeof candidate, "function", "isOngoingDispatch must be exported");
  const isOngoingDispatch = candidate as (status: string) => boolean;

  for (const status of [
    "queued",
    "starting",
    "running",
    "awaiting_approval",
    "interrupting",
    "idle",
    "shutting_down",
  ]) {
    assert.equal(isOngoingDispatch(status), true, `${status} should remain visible`);
  }

  for (const status of [
    "shut_down",
    "completed",
    "failed",
    "cancelled",
    "interrupted",
  ]) {
    assert.equal(isOngoingDispatch(status), false, `${status} should leave ongoing work`);
  }
});
