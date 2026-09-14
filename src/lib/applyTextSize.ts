import { LogicalSize, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import {
  TEXT_SIZES,
  TEXT_SIZE_STORAGE_KEY,
  minWindowSizeFor,
  parseTextSize,
  type TextSize,
} from "./textSize";

/**
 * The preference lives in localStorage rather than SQLite: it must be readable synchronously
 * in `main.tsx` before the first render, so the UI never paints at one size and jumps to
 * another. Storage can throw (e.g. disabled WebKit storage) — then the default is used.
 */
export function readStoredTextSize(): TextSize {
  try {
    return parseTextSize(window.localStorage.getItem(TEXT_SIZE_STORAGE_KEY));
  } catch {
    return parseTextSize(null);
  }
}

export function storeTextSize(size: TextSize) {
  try {
    window.localStorage.setItem(TEXT_SIZE_STORAGE_KEY, size);
  } catch {
    // Not persisted; still applied for this launch.
  }
}

/** Synchronous and DOM-only, so it can run before React mounts. */
export function applyRootTextSize(size: TextSize) {
  document.documentElement.style.fontSize = `${TEXT_SIZES[size].rootPx}px`;
}

/**
 * Raises (or restores) the window's minimum size to fit the chosen text size, growing the
 * window if it is currently smaller. Failures are logged, not thrown: the text size change
 * itself has already applied and is still usable in a window that can be resized too small.
 */
export async function applyWindowMinSize(size: TextSize) {
  try {
    const appWindow = getCurrentWindow();
    const monitor = await currentMonitor();
    const screen = monitor
      ? monitor.workArea.size.toLogical(monitor.scaleFactor)
      : null;
    const min = minWindowSizeFor(size, screen);
    await appWindow.setMinSize(new LogicalSize(min.width, min.height));

    const scaleFactor = await appWindow.scaleFactor();
    const current = (await appWindow.innerSize()).toLogical(scaleFactor);
    if (current.width < min.width || current.height < min.height) {
      await appWindow.setSize(
        new LogicalSize(Math.max(current.width, min.width), Math.max(current.height, min.height)),
      );
    }
  } catch (error) {
    console.warn("could not update the minimum window size for text size", error);
  }
}

export function applyTextSize(size: TextSize) {
  applyRootTextSize(size);
  void applyWindowMinSize(size);
}
