import assert from "node:assert/strict";
import test from "node:test";

import {
  BASE_MIN_WINDOW,
  DEFAULT_TEXT_SIZE,
  TEXT_SIZES,
  minWindowSizeFor,
  parseTextSize,
} from "../src/lib/textSize.ts";

test("stored preferences fall back to the default when missing or unknown", () => {
  assert.equal(parseTextSize(null), DEFAULT_TEXT_SIZE);
  assert.equal(parseTextSize("gigantic"), DEFAULT_TEXT_SIZE);
  assert.equal(parseTextSize("large"), "large");
});

test("the default size keeps the original minimum window", () => {
  assert.equal(TEXT_SIZES[DEFAULT_TEXT_SIZE].rootPx, 16);
  assert.deepEqual(minWindowSizeFor(DEFAULT_TEXT_SIZE, null), BASE_MIN_WINDOW);
});

test("larger text raises the minimum window proportionally", () => {
  const large = minWindowSizeFor("large", null);
  const scale = TEXT_SIZES.large.rootPx / 16;
  assert.deepEqual(large, {
    width: Math.round(BASE_MIN_WINDOW.width * scale),
    height: Math.round(BASE_MIN_WINDOW.height * scale),
  });
});

test("the minimum never exceeds the screen it is on", () => {
  const smallScreen = { width: 1280, height: 800 };
  const size = minWindowSizeFor("xlarge", smallScreen);
  assert.ok(size.width <= smallScreen.width);
  assert.ok(size.height <= smallScreen.height);
  // ...but never drops below the default minimum either.
  assert.ok(size.width >= BASE_MIN_WINDOW.width);
  assert.ok(size.height >= BASE_MIN_WINDOW.height);
});

test("smaller text never lowers the minimum below the default layout floor", () => {
  assert.deepEqual(minWindowSizeFor("small", null), BASE_MIN_WINDOW);
});
