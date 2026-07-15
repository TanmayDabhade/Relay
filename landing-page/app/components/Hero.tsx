import Image from "next/image";
import heroImage from "@/images/hero.png";
import { CliMarquee } from "./CliMarquee";

const TAGS = ["Local-first", "Every agent", "Zero telemetry"];

export function Hero() {
  return (
    <section id="top" className="mx-auto max-w-6xl px-6 pt-16 pb-20 sm:pt-24">
      <div className="grid gap-12 lg:grid-cols-2 lg:items-center lg:gap-16">
        <div>
          <span className="inline-block border border-relay-border-strong px-3 py-1 text-xs tracking-widest text-relay-muted">
            v0.1 · PRIVATE BETA
          </span>
          <h1 className="mt-6 font-display text-4xl leading-[1.05] font-black tracking-tight uppercase sm:text-5xl lg:text-6xl">
            The control plane for your{" "}
            <span className="text-relay-primary">coding agents.</span>
          </h1>
          <p className="mt-6 max-w-xl text-base leading-relaxed text-relay-muted">
            Relay isn&apos;t another dashboard — it&apos;s where you{" "}
            <span className="text-relay-text">run</span> the work. Plan tasks on
            a Kanban, launch real terminal sessions straight from a card, and
            drive every agent —{" "}
            <code className="text-relay-text">.claude</code>,{" "}
            <code className="text-relay-text">.codex</code>,{" "}
            <code className="text-relay-text">.cursor</code>,{" "}
            <code className="text-relay-text">.gemini</code> — from one board.
            Cost and session tracking come standard. All on your machine, zero
            setup.
          </p>
          <div className="mt-8 flex flex-wrap items-center gap-6">
            <a
              href="#waitlist"
              className="border border-relay-primary bg-relay-primary px-6 py-3 text-sm font-bold text-relay-primary-contrast transition-opacity hover:opacity-90"
            >
              $ Join Waitlist
            </a>
            <ul className="flex flex-wrap gap-x-5 gap-y-2 text-xs text-relay-muted">
              {TAGS.map((t) => (
                <li key={t}>{`// ${t}`}</li>
              ))}
            </ul>
          </div>
        </div>
        <div className="space-y-2">
        <Image src={heroImage} alt="Relay Dashboard" className="w-full rounded-lg" priority />
        </div>
      </div>
      <div className="mt-16">
        <CliMarquee />
      </div>
    </section>
  );
}
