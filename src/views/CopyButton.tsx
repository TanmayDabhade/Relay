import { useEffect, useRef, useState } from "react";
import { Button } from "../components/ui/Button";
import { copyText } from "../lib/clipboard";
import "./CopyButton.css";

interface CopyButtonProps {
  getText: () => string | Promise<string>;
  label: string;
  variant?: "primary" | "secondary";
}

type CopyStatus = "idle" | "copying" | "copied" | "error";

const STATUS_LABEL: Record<Exclude<CopyStatus, "idle">, string> = {
  copying: "Copying…",
  copied: "Copied",
  error: "Copy failed",
};

export function CopyButton({ getText, label, variant = "primary" }: CopyButtonProps) {
  const [status, setStatus] = useState<CopyStatus>("idle");
  const resetTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (resetTimer.current !== null) {
        clearTimeout(resetTimer.current);
      }
    };
  }, []);

  async function handleCopy() {
    if (resetTimer.current !== null) {
      clearTimeout(resetTimer.current);
      resetTimer.current = null;
    }
    setStatus("copying");

    try {
      await copyText(await getText());
      if (!mounted.current) return;

      setStatus("copied");
      resetTimer.current = setTimeout(() => {
        setStatus("idle");
        resetTimer.current = null;
      }, 2_000);
    } catch {
      if (mounted.current) {
        setStatus("error");
      }
    }
  }

  const statusLabel = status === "idle" ? label : STATUS_LABEL[status];

  return (
    <span className="copy-button-shell">
      <Button
        className="copy-button"
        variant={variant}
        onClick={handleCopy}
        disabled={status === "copying"}
      >
        {statusLabel}
      </Button>
      <span className="copy-button-status" aria-live="polite" aria-atomic="true">
        {status === "idle" ? "" : statusLabel}
      </span>
    </span>
  );
}
