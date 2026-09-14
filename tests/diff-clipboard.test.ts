import assert from "node:assert/strict";
import test from "node:test";

import { formatDiffForClipboard } from "../src/views/diffClipboard.ts";

test("formatDiffForClipboard preserves prefixes and warns when content is truncated", () => {
  const result = formatDiffForClipboard({
    lines: [
      { tag: "equal", content: "context" },
      { tag: "delete", content: "old value" },
      { tag: "insert", content: "new value" },
    ],
    truncated: true,
    occurred_at: 1_700_000_000,
    edit_count: 2,
  });

  assert.equal(
    result,
    " context\n-old value\n+new value\n\n[diff truncated — too large to show in full]",
  );
});
