/**
 * Display-side counterpart of `src-tauri/src/dispatch/looping.rs`. The backend stores the exact
 * text exchanged with the agent (so transcripts stay truthful); these helpers keep the loop's
 * machinery — the stopping contract, continuation prompts, and the done marker — out of the
 * chat the user reads. The strings here must match the Rust prompt builders.
 */

export const LOOP_DONE_MARKER = "RELAY_LOOP_DONE";

const CONTRACT_START = "\n\n---\nRelay is running this task in a loop:";
const CONTINUATION = /^Continue working on the task \(loop iteration (\d+) of (\d+)\)\./;
/** Shortest trailing fragment treated as a marker still streaming in. */
const MIN_PARTIAL_MARKER = 4;

export function stripLoopContract(prompt: string): { text: string; looping: boolean } {
  const index = prompt.indexOf(CONTRACT_START);
  if (index === -1) return { text: prompt, looping: false };
  return { text: prompt.slice(0, index).trimEnd(), looping: true };
}

export function parseLoopContinuation(prompt: string): { iteration: number; max: number } | null {
  const match = CONTINUATION.exec(prompt);
  if (!match) return null;
  return { iteration: Number(match[1]), max: Number(match[2]) };
}

export function stripLoopDoneMarker(message: string): { text: string; done: boolean } {
  if (message.includes(LOOP_DONE_MARKER)) {
    return { text: message.replaceAll(LOOP_DONE_MARKER, "").trim(), done: true };
  }
  // While streaming, the marker arrives a few characters at a time; hide a trailing fragment
  // of it so "RELAY_LOO" never flashes on screen before the full word lands.
  const trailing = /(?:^|\s)([A-Z_]+)$/.exec(message);
  const fragment = trailing?.[1];
  if (
    fragment &&
    fragment.length >= MIN_PARTIAL_MARKER &&
    LOOP_DONE_MARKER.startsWith(fragment)
  ) {
    return { text: message.slice(0, message.length - fragment.length).trimEnd(), done: false };
  }
  return { text: message, done: false };
}
