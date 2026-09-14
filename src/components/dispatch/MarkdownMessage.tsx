import { useEffect, useRef, useState } from "react";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { copyText } from "../../lib/clipboard";
import { openUrl } from "../../lib/tauri";

/** Only schemes that `open` hands to a browser or mail client — never file:, app, or custom URLs. */
const EXTERNAL_LINK = /^(https?:|mailto:)/i;

const COMPONENTS: Components = {
  // A plain <a> would navigate the app's own webview away from Relay; open externally instead.
  a({ href, children }) {
    return (
      <a
        href={href}
        onClick={(event) => {
          event.preventDefault();
          if (href && EXTERNAL_LINK.test(href)) void openUrl(href);
        }}
      >
        {children}
      </a>
    );
  },
  // Wide agent tables scroll inside the message instead of stretching the chat column.
  table({ children }) {
    return (
      <div className="chat-markdown-table">
        <table>{children}</table>
      </div>
    );
  },
};

/**
 * Agent replies are GitHub-flavored Markdown (tables, nested lists, code). Rendered with
 * react-markdown, which builds React elements and ignores raw HTML by default — agent output
 * is untrusted text, so nothing in it can inject markup or script into the app.
 */
export function MarkdownMessage({ text }: { text: string }) {
  return (
    <div className="chat-markdown">
      <Markdown remarkPlugins={[remarkGfm]} components={COMPONENTS}>
        {text}
      </Markdown>
    </div>
  );
}

/** Compact header action that copies one response's Markdown source. */
export function CopyResponseButton({ text }: { text: string }) {
  const [status, setStatus] = useState<"idle" | "copied" | "error">("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current !== null) clearTimeout(timer.current);
    },
    [],
  );

  async function copy() {
    if (timer.current !== null) clearTimeout(timer.current);
    try {
      await copyText(text);
      setStatus("copied");
    } catch {
      setStatus("error");
    }
    timer.current = setTimeout(() => setStatus("idle"), 2_000);
  }

  return (
    <button
      type="button"
      className={`chat-copy-button${status === "copied" ? " is-copied" : ""}`}
      onClick={() => void copy()}
      aria-live="polite"
    >
      {status === "copied" ? "Copied" : status === "error" ? "Copy failed" : "Copy"}
    </button>
  );
}
