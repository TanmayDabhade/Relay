import type { DiffLine, FileDiff } from "../lib/types";

export const DIFF_TAG_PREFIX: Record<DiffLine["tag"], string> = {
  insert: "+",
  delete: "-",
  equal: " ",
};

export function formatDiffForClipboard(data: FileDiff): string {
  const diff = data.lines
    .map((line) => `${DIFF_TAG_PREFIX[line.tag]}${line.content}`)
    .join("\n");

  return data.truncated
    ? `${diff}\n\n[diff truncated — too large to show in full]`
    : diff;
}
