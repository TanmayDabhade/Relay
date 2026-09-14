import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { CopyResponseButton, MarkdownMessage } from "./MarkdownMessage";
import {
  parseLoopContinuation,
  stripLoopContract,
  stripLoopDoneMarker,
} from "../../lib/loopMarkers";
import { getDispatchConversation, resolveDispatchApproval } from "../../lib/tauri";
import type {
  DispatchApprovalDecision,
  DispatchConversation,
  DispatchEvent,
} from "../../lib/types";

interface RunChatProps {
  runId: string;
  /** True while the agent is mid-turn; the trailing tool group shows "Working". */
  active?: boolean;
}

type ChatItem =
  | { type: "event"; event: DispatchEvent }
  | { type: "tools"; events: DispatchEvent[] };

function isToolEvent(event: DispatchEvent): boolean {
  return event.kind === "tool_call" || event.kind === "tool_result";
}

// Collapse each run of consecutive tool calls/results into one group.
function groupEvents(events: DispatchEvent[]): ChatItem[] {
  const items: ChatItem[] = [];
  for (const event of events) {
    if (event.kind === "legacy_output") continue;
    const last = items[items.length - 1];
    if (isToolEvent(event)) {
      if (last?.type === "tools") last.events.push(event);
      else items.push({ type: "tools", events: [event] });
    } else {
      items.push({ type: "event", event });
    }
  }
  return items;
}

function durationLabel(seconds: number): string {
  if (seconds < 60) return `${Math.max(seconds, 1)}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return rest ? `${minutes}m ${rest}s` : `${minutes}m`;
}

function WorkingLabel() {
  return (
    <span className="chat-working" role="status">
      Working
    </span>
  );
}

function ToolGroup({ events, working }: { events: DispatchEvent[]; working: boolean }) {
  const calls = events.filter((event) => event.kind === "tool_call").length || events.length;
  const errors = events.filter((event) => event.state === "error").length;
  const seconds = events[events.length - 1].created_at - events[0].created_at;
  return (
    <details className="chat-tool-group">
      <summary>
        {working ? (
          <WorkingLabel />
        ) : (
          <span className="chat-tool-group-label">
            Worked for {durationLabel(seconds)}
          </span>
        )}
        <span className="chat-tool-group-meta">
          {calls} {calls === 1 ? "step" : "steps"}
          {errors ? ` · ${errors} failed` : ""}
        </span>
        <span className="chat-tool-caret" aria-hidden>›</span>
      </summary>
      <div className="chat-tool-group-body">
        {events.map((event) => (
          <ToolEvent key={event.id} event={event} />
        ))}
      </div>
    </details>
  );
}

function eventTime(epoch: number): string {
  return new Date(epoch * 1000).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });
}

function eventDetails(event: DispatchEvent): string {
  if (!event.payload) return event.content;
  try {
    return JSON.stringify(JSON.parse(event.payload), null, 2);
  } catch {
    return event.payload;
  }
}

function ToolEvent({ event }: { event: DispatchEvent }) {
  const result = event.kind === "tool_result";
  return (
    <details className={`chat-tool-row${event.state === "error" ? " is-error" : ""}`}>
      <summary>
        <span className="chat-tool-caret" aria-hidden>›</span>
        <span className="chat-tool-kind">{result ? "Result" : "Tool"}</span>
        <strong>{event.content || "Agent tool"}</strong>
        <span className={`chat-event-state state-${event.state ?? "completed"}`}>
          {event.state ?? (result ? "completed" : "running")}
        </span>
        <time>{eventTime(event.created_at)}</time>
      </summary>
      <pre>{eventDetails(event)}</pre>
    </details>
  );
}

interface ApprovalEventProps {
  event: DispatchEvent;
  busy: boolean;
  onDecision: (decision: DispatchApprovalDecision) => void;
}

function ApprovalEvent({ event, busy, onDecision }: ApprovalEventProps) {
  const pending = event.state === "pending";
  return (
    <details className="chat-approval-row" open={pending || undefined}>
      <summary>
        <span className="chat-approval-mark" aria-hidden>!</span>
        <strong>{pending ? "Approval needed" : "Approval resolved"}</strong>
        <span className={`chat-event-state state-${event.state ?? "pending"}`}>
          {(event.state ?? "pending").replaceAll("_", " ")}
        </span>
        <time>{eventTime(event.created_at)}</time>
      </summary>
      <div className="chat-approval-body">
        <p>{event.content}</p>
        <pre>{eventDetails(event)}</pre>
        {pending ? (
          <div className="chat-approval-actions">
            <button disabled={busy} onClick={() => onDecision("allowed_once")}>
              Allow once
            </button>
            <button disabled={busy} onClick={() => onDecision("allowed_for_session")}>
              Always allow for session
            </button>
            <button
              className="is-deny"
              disabled={busy}
              onClick={() => onDecision("denied")}
            >
              Deny
            </button>
          </div>
        ) : null}
      </div>
    </details>
  );
}

function appendLiveEvent(
  conversation: DispatchConversation | null | undefined,
  event: DispatchEvent,
): DispatchConversation | null | undefined {
  if (!conversation || event.run_id !== conversation.run.id) return conversation;
  if (conversation.events.some((existing) => existing.id === event.id)) return conversation;
  return {
    ...conversation,
    events: [...conversation.events, event].sort((left, right) => left.sequence - right.sequence),
  };
}

export function RunChat({ runId, active = false }: RunChatProps) {
  const queryClient = useQueryClient();
  const scrollRef = useRef<HTMLDivElement>(null);
  const followOutputRef = useRef(true);
  const [busyApproval, setBusyApproval] = useState<number | null>(null);
  const [approvalError, setApprovalError] = useState<string | null>(null);
  const { data: conversation, isLoading, isError } = useQuery({
    queryKey: ["dispatch-conversation", runId],
    queryFn: () => getDispatchConversation(runId),
  });

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen<DispatchEvent>("dispatch-event", (incoming) => {
      if (incoming.payload.run_id !== runId) return;
      queryClient.setQueryData<DispatchConversation | null>(
        ["dispatch-conversation", runId],
        (current) => appendLiveEvent(current, incoming.payload) ?? null,
      );
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient, runId]);

  const events = conversation?.events ?? [];
  const legacyText = events
    .filter((event) => event.kind === "legacy_output")
    .map((event) => event.content)
    .join("");
  const items = groupEvents(events);

  useEffect(() => {
    const host = scrollRef.current;
    if (!host || !followOutputRef.current) return;
    host.scrollTop = host.scrollHeight;
  }, [events.length]);

  async function decide(event: DispatchEvent, decision: DispatchApprovalDecision) {
    setBusyApproval(event.id);
    setApprovalError(null);
    try {
      await resolveDispatchApproval(runId, event.id, decision);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["dispatch-conversation", runId] }),
        queryClient.invalidateQueries({ queryKey: ["dispatch-tasks"] }),
      ]);
    } catch (reason) {
      setApprovalError(String(reason));
    } finally {
      setBusyApproval(null);
    }
  }

  return (
    <div
      className="run-chat"
      ref={scrollRef}
      aria-label="Agent conversation"
      onScroll={(event) => {
        const host = event.currentTarget;
        followOutputRef.current = host.scrollHeight - host.scrollTop - host.clientHeight < 72;
      }}
    >
      {isLoading ? <p className="run-chat-empty">Loading conversation…</p> : null}
      {isError ? <p className="run-chat-empty">The conversation could not be loaded.</p> : null}
      {!isLoading && !isError && events.length === 0 ? (
        <p className="run-chat-empty">Waiting for the agent’s first structured event…</p>
      ) : null}

      {conversation?.legacy ? (
        <section className="chat-legacy-transcript">
          <span>Legacy terminal transcript</span>
          <pre>{legacyText || "No captured output for this run."}</pre>
        </section>
      ) : (
        <div className="chat-event-list">
          {items.map((item, index) => {
            const trailing = index === items.length - 1;
            if (item.type === "tools") {
              return (
                <ToolGroup
                  key={item.events[0].id}
                  events={item.events}
                  working={active && trailing}
                />
              );
            }
            const { event } = item;
            if (event.kind === "user_message" || event.kind === "assistant_message") {
              const user = event.kind === "user_message";
              // Relay's own loop prompts aren't the user's words: show them as a divider.
              const continuation = user ? parseLoopContinuation(event.content) : null;
              if (continuation) {
                return (
                  <div key={event.id} className="chat-loop-divider" role="separator">
                    <span>
                      Loop · continuing {continuation.iteration} of {continuation.max}
                    </span>
                    <time>{eventTime(event.created_at)}</time>
                  </div>
                );
              }
              const prompt = user ? stripLoopContract(event.content) : null;
              const reply = user ? null : stripLoopDoneMarker(event.content);
              const text = prompt?.text ?? reply?.text ?? event.content;
              return (
                <article
                  key={event.id}
                  className={`chat-message ${user ? "is-user" : "is-assistant"}`}
                >
                  <header>
                    <span>{user ? "You" : "Agent"}</span>
                    {prompt?.looping ? <span className="chat-loop-tag">loop until done</span> : null}
                    {reply?.done ? (
                      <span className="chat-loop-tag is-done">✓ reported done</span>
                    ) : null}
                    <time>{eventTime(event.created_at)}</time>
                  </header>
                  {user ? <p>{text}</p> : <MarkdownMessage text={text} />}
                  {!user && event.state !== "running" && text ? (
                    <footer className="chat-message-actions">
                      <CopyResponseButton text={text} />
                    </footer>
                  ) : null}
                </article>
              );
            }
            if (event.kind === "approval_request") {
              return (
                <ApprovalEvent
                  key={event.id}
                  event={event}
                  busy={busyApproval === event.id}
                  onDecision={(decision) => void decide(event, decision)}
                />
              );
            }
            return (
              <div key={event.id} className={`chat-notice is-${event.kind}`}>
                <span>
                  {event.kind === "error"
                    ? "Error"
                    : event.content.startsWith("Loop ")
                      ? "Loop"
                      : event.state ?? "Update"}
                </span>
                <p>{event.content}</p>
              </div>
            );
          })}
          {active && items[items.length - 1]?.type !== "tools" ? (
            <div className="chat-working-row">
              <WorkingLabel />
            </div>
          ) : null}
        </div>
      )}
      {approvalError ? <p className="run-action-error">{approvalError}</p> : null}
    </div>
  );
}
