"use client";

import { useEffect, useState } from "react";

type Line = { prompt: string; output: string };

const LINES: Line[] = [
  {
    prompt: "relay scan",
    output: "→ found .claude, .codex, .cursor, .gemini",
  },
  {
    prompt: "relay spawn build-auth-flow",
    output: "→ agent[claude-code] booted · pid 42081",
  },
  {
    prompt: "relay spend --today",
    output: "→ $2.14 across 6 sessions ($0.36 avg)",
  },
];

const TYPE_MS = 35;
const HOLD_MS = 1400;
const OUTPUT_DELAY_MS = 250;

export function TerminalWindow() {
  const [lineIndex, setLineIndex] = useState(0);
  const [typed, setTyped] = useState("");
  const [showOutput, setShowOutput] = useState(false);
  const [history, setHistory] = useState<Line[]>([]);
  const [reduceMotion, setReduceMotion] = useState(
    () =>
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const handler = () => setReduceMotion(mq.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  useEffect(() => {
    if (reduceMotion) return;
    const current = LINES[lineIndex];

    if (typed.length < current.prompt.length) {
      const t = setTimeout(
        () => setTyped(current.prompt.slice(0, typed.length + 1)),
        TYPE_MS,
      );
      return () => clearTimeout(t);
    }

    if (!showOutput) {
      const t = setTimeout(() => setShowOutput(true), OUTPUT_DELAY_MS);
      return () => clearTimeout(t);
    }

    const t = setTimeout(() => {
      const next = (lineIndex + 1) % LINES.length;
      setHistory((h) => (next === 0 ? [] : [...h, current]));
      setLineIndex(next);
      setTyped("");
      setShowOutput(false);
    }, HOLD_MS);
    return () => clearTimeout(t);
  }, [typed, showOutput, lineIndex, reduceMotion]);

  return (
    <div className="w-full border border-relay-border bg-relay-surface text-sm">
      <div className="flex items-center gap-2 border-b border-relay-border px-4 py-2.5">
        <span className="h-2.5 w-2.5 rounded-full bg-relay-red/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-relay-gold/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-relay-green/70" />
        <span className="ml-2 text-xs text-relay-muted">relay // ~/dev shell</span>
      </div>
      <div className="min-h-[190px] space-y-1.5 p-4">
        {(reduceMotion ? LINES : history).map((line, i) => (
          <div key={`${line.prompt}-${i}`}>
            <p>
              <span className="text-relay-primary">~/relay $</span> {line.prompt}
            </p>
            <p className="text-relay-green">{line.output}</p>
          </div>
        ))}
        {!reduceMotion && (
          <div>
            <p>
              <span className="text-relay-primary">~/relay $</span> {typed}
              <span className="ml-0.5 animate-pulse">▍</span>
            </p>
            {showOutput && (
              <p className="text-relay-green">{LINES[lineIndex].output}</p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
