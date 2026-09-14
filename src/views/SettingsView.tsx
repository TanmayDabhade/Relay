import { useState } from "react";
import { applyTextSize, readStoredTextSize, storeTextSize } from "../lib/applyTextSize";
import { TEXT_SIZES, type TextSize } from "../lib/textSize";
import "./SettingsView.css";

export function SettingsView() {
  const [textSize, setTextSize] = useState<TextSize>(readStoredTextSize);

  function choose(size: TextSize) {
    setTextSize(size);
    storeTextSize(size);
    applyTextSize(size);
  }

  return (
    <div className="settings-view">
      <header className="settings-header">
        <h1>Settings</h1>
      </header>

      <section className="settings-section" aria-labelledby="settings-appearance">
        <h2 id="settings-appearance">Appearance</h2>
        <div className="settings-row">
          <div className="settings-row-label">
            <strong>Text size</strong>
            <p>
              Applies to the whole interface and stays the same at every window size. Larger
              sizes raise the smallest size the window can be resized to.
            </p>
          </div>
          <div className="settings-segmented" role="radiogroup" aria-label="Text size">
            {(Object.keys(TEXT_SIZES) as TextSize[]).map((size) => (
              <button
                key={size}
                type="button"
                role="radio"
                aria-checked={textSize === size}
                className={textSize === size ? "is-selected" : undefined}
                onClick={() => choose(size)}
              >
                <span style={{ fontSize: `${TEXT_SIZES[size].rootPx * 0.8125}px` }}>Aa</span>
                {TEXT_SIZES[size].label}
              </button>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}
