/**
 * User-selectable interface text size. Every spacing and type token is rem, so the root font
 * size alone scales the whole UI. It used to follow window width; it is now a fixed,
 * user-chosen value, like a native macOS app: resizing the window never changes text size.
 *
 * Pure (no Tauri or DOM access) so it runs under `node --test`; `applyTextSize.ts` does the
 * side effects.
 */

export type TextSize = "small" | "default" | "large" | "xlarge";

export const TEXT_SIZES: Record<TextSize, { label: string; rootPx: number }> = {
  small: { label: "Small", rootPx: 14 },
  default: { label: "Default", rootPx: 16 },
  large: { label: "Large", rootPx: 18 },
  xlarge: { label: "Extra large", rootPx: 20 },
};

export const DEFAULT_TEXT_SIZE: TextSize = "default";
export const TEXT_SIZE_STORAGE_KEY = "relay.textSize";

/**
 * The smallest window the layout is designed for at the default text size. Keep in sync with
 * `minWidth`/`minHeight` in `src-tauri/tauri.conf.json`, which applies it before JS loads.
 */
export const BASE_MIN_WINDOW = { width: 1040, height: 660 };

export function parseTextSize(stored: string | null): TextSize {
  return stored != null && stored in TEXT_SIZES ? (stored as TextSize) : DEFAULT_TEXT_SIZE;
}

/**
 * Larger text needs proportionally more room before the layout breaks, so the minimum
 * window grows with it. It is capped at the screen size (a minimum larger than the display
 * would make the window impossible to fit), and never shrinks below the default floor —
 * smaller text doesn't make the sidebar and panels any narrower.
 */
export function minWindowSizeFor(
  size: TextSize,
  screen: { width: number; height: number } | null,
): { width: number; height: number } {
  const scale = Math.max(1, TEXT_SIZES[size].rootPx / TEXT_SIZES[DEFAULT_TEXT_SIZE].rootPx);
  let width = Math.round(BASE_MIN_WINDOW.width * scale);
  let height = Math.round(BASE_MIN_WINDOW.height * scale);
  if (screen) {
    width = Math.max(BASE_MIN_WINDOW.width, Math.min(width, screen.width));
    height = Math.max(BASE_MIN_WINDOW.height, Math.min(height, screen.height));
  }
  return { width, height };
}
