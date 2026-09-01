import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { getDispatchConversation, resolveDispatchApproval } from "../../lib/tauri";
import type {
  DispatchApprovalDecision,
  DispatchConversation,
  DispatchEvent,
} from "../../lib/types";

interface RunChatProps {
  runId: string;
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

export function RunChat({ runId }: RunChatProps) {
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
          {events.map((event) => {
            if (event.kind === "user_message" || event.kind === "assistant_message") {
              const user = event.kind === "user_message";
              return (
                <article
                  key={event.id}
                  className={`chat-message ${user ? "is-user" : "is-assistant"}`}
                >
                  <header>
                    <span>{user ? "You" : "Agent"}</span>
                    <time>{eventTime(event.created_at)}</time>
                  </header>
                  <p>{event.content}</p>
                </article>
              );
            }
            if (event.kind === "tool_call" || event.kind === "tool_result") {
              return <ToolEvent key={event.id} event={event} />;
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
            if (event.kind === "legacy_output") return null;
            return (
              <div key={event.id} className={`chat-notice is-${event.kind}`}>
                <span>{event.kind === "error" ? "Error" : event.state ?? "Update"}</span>
                <p>{event.content}</p>
              </div>
            );
          })}
        </div>
      )}
      {approvalError ? <p className="run-action-error">{approvalError}</p> : null}
    </div>
  );
}
