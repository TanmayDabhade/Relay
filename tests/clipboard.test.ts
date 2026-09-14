import assert from "node:assert/strict";
import test from "node:test";

import { copyText } from "../src/lib/clipboard.ts";

test("copyText falls back to a temporary textarea when Clipboard API access fails", async () => {
  const events: string[] = [];
  const textarea = {
    value: "",
    style: {} as CSSStyleDeclaration,
    setAttribute(name: string, value: string) {
      events.push(`attribute:${name}=${value}`);
    },
    select() {
      events.push("select");
    },
    remove() {
      events.push("remove");
    },
  };
  const documentStub = {
    createElement(tagName: string) {
      events.push(`create:${tagName}`);
      return textarea;
    },
    body: {
      appendChild() {
        events.push("append");
      },
    },
    execCommand(command: string) {
      events.push(`exec:${command}`);
      return true;
    },
  } as unknown as Document;
  const clipboardStub = {
    async writeText() {
      events.push("clipboard");
      throw new Error("not allowed in this webview");
    },
  } as Clipboard;

  await copyText("Relay transcript", clipboardStub, documentStub);

  assert.equal(textarea.value, "Relay transcript");
  assert.deepEqual(events, [
    "clipboard",
    "create:textarea",
    "attribute:readonly=",
    "append",
    "select",
    "exec:copy",
    "remove",
  ]);
});
