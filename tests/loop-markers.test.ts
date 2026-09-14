import assert from "node:assert/strict";
import test from "node:test";

import {
  LOOP_DONE_MARKER,
  parseLoopContinuation,
  stripLoopContract,
  stripLoopDoneMarker,
} from "../src/lib/loopMarkers.ts";

// Must stay byte-identical to `looping::initial_prompt` in src-tauri/src/dispatch/looping.rs.
const CONTRACT =
  "\n\n---\nRelay is running this task in a loop: after each of your turns it will ask you to " +
  "continue. When the task is fully complete and verified, end your final message with " +
  `${LOOP_DONE_MARKER} on its own line. Do not write that marker before then.`;

test("the loop contract is hidden from the user's first prompt", () => {
  assert.deepEqual(stripLoopContract(`Rewrite the README.${CONTRACT}`), {
    text: "Rewrite the README.",
    looping: true,
  });
  assert.deepEqual(stripLoopContract("A normal prompt"), {
    text: "A normal prompt",
    looping: false,
  });
});

test("continuation prompts are recognized with their iteration", () => {
  const prompt =
    "Continue working on the task (loop iteration 2 of 5). Review what is left, keep going, " +
    `and verify your work. If everything is complete and verified, end your final message ` +
    `with ${LOOP_DONE_MARKER} on its own line.`;
  assert.deepEqual(parseLoopContinuation(prompt), { iteration: 2, max: 5 });
  assert.equal(parseLoopContinuation("Continue working on the task please"), null);
});

test("the done marker is removed from the agent's message and reported", () => {
  assert.deepEqual(stripLoopDoneMarker(`All finished.\n\n${LOOP_DONE_MARKER}`), {
    text: "All finished.",
    done: true,
  });
  assert.deepEqual(stripLoopDoneMarker(`Done ${LOOP_DONE_MARKER} `), {
    text: "Done",
    done: true,
  });
  assert.deepEqual(stripLoopDoneMarker("Still working"), { text: "Still working", done: false });
});

test("a marker still arriving mid-stream never flashes as partial text", () => {
  assert.deepEqual(stripLoopDoneMarker("All finished.\n\nRELAY_LOO"), {
    text: "All finished.",
    done: false,
  });
  // An ordinary word that merely starts with "R" is not a marker prefix to hide.
  assert.deepEqual(stripLoopDoneMarker("Ran the tests. R"), {
    text: "Ran the tests. R",
    done: false,
  });
});
