# Structured Agent Chat Runtime

**Date:** 2026-08-26  
**Status:** Approved in chat; pending written-spec review

## Purpose

Replace Relay's embedded terminal reproduction with a native chat experience for Relay-owned CLI agent sessions. The runtime must preserve the capabilities that required an interactive PTY—multi-turn prompts, tool visibility, approval decisions, interruption, and shutdown—without exposing ANSI terminal UI or duplicating the CLI's prompt editor.

This change covers the four built-in agents: Claude Code, Codex CLI, Gemini CLI, and Cursor Agent.

## User-visible contract

- A dispatched task is a persistent conversation, not a one-shot terminal process.
- The initial task prompt appears as the first user message.
- Assistant prose streams into assistant messages.
- Tool calls and results appear chronologically as compact, expandable rows.
- Approval requests appear as expandable rows with **Allow once**, **Always allow for session**, and **Deny** actions.
- **Stop turn** interrupts only the active turn. It does not close the conversation or disable the composer.
- After a turn completes normally or is interrupted, the conversation becomes idle and the composer stays enabled.
- A later prompt resumes the same native agent conversation with its prior context.
- **Shut down** is the only user action that closes the conversation and disables further prompts.
- A Relay application restart must not erase a conversation. If the provider supports durable resume, a new prompt reattaches using the stored native session identifier. If the provider cannot resume, Relay displays a clear conversation error instead of silently starting without context.

## Architecture

### Normalized runtime

The current `dispatch::Runtime` owns a PTY master, writer, and process killer and emits raw output chunks. Replace that terminal-specific API with a conversation runtime whose public operations are:

- `start_conversation`
- `send_prompt`
- `interrupt_turn`
- `resolve_approval`
- `shutdown_conversation`

The runtime owns active provider drivers in memory. Durable conversation state, native provider identifiers, and normalized transcript events remain in SQLite. A provider process may exit between turns; the Relay conversation remains logically open and resumable until explicit shutdown.

Each provider driver implements the same behavioral contract:

1. Start or resume a provider conversation.
2. Translate a user prompt into the provider's protocol.
3. Stream provider events into Relay's normalized event model.
4. Surface approval requests and route decisions back to the exact pending provider request.
5. Interrupt only the current provider turn.
6. Shut down active resources without deleting durable history.

Provider-specific transport details stay behind the driver boundary:

- **Claude Code:** bidirectional stream JSON.
- **Codex CLI:** app-server JSON-RPC, including native turn interruption and approval requests.
- **Gemini CLI:** ACP where available, with structured stream/resume as the version-gated fallback.
- **Cursor Agent:** structured stream JSON plus its native chat identifier and resume flow.

The runtime validates capabilities at launch. A CLI version missing a required structured capability fails with an actionable connection error; Relay does not fall back to scraping terminal screens for new conversations.

### Conversation lifecycle

The normalized lifecycle is:

```text
starting -> running -> awaiting_approval -> running
                    -> idle
                    -> failed

running --interrupt--> interrupting -> idle
idle --send prompt--> running
idle/running/failed --shut down--> shutting_down -> shut_down
```

`awaiting_approval` is an active-turn state. Denying an approval follows the provider's native behavior; if that decision interrupts the turn, Relay settles in `idle`, not `shut_down`.

Only one active turn may exist per conversation. Sending while `running`, `interrupting`, or `awaiting_approval` is rejected with a clear message. The composer is enabled in `idle` and `failed` when the provider session is resumable, and disabled only during startup/transitions or after shutdown.

Runtime exit handling distinguishes intent:

- A provider process ending after a successful turn moves the conversation to `idle`.
- An interrupted turn moves the conversation to `idle` once interruption is acknowledged or the child exits.
- An unexpected provider failure moves the conversation to `failed` but preserves resume capability when a native session identifier exists.
- Explicit shutdown moves the conversation to `shut_down` and records the close time.
- Relay closing while a turn is active marks that turn interrupted. The conversation itself remains resumable after restart unless it had already been shut down.

### Persistence

Add a new migration; do not edit migration `0007_dispatch.sql`.

Keep `dispatch_tasks` as the durable user intent and workday entry. Treat `dispatch_runs` as the durable provider conversation record for compatibility with existing queries and card/session linking. Extend it with the provider session identifier and conversation lifecycle timestamps/state required for resume and shutdown.

Add normalized turn and event storage:

- `dispatch_turns`: one row per user turn, with status, timing, interruption/error data, and the provider turn identifier when available.
- `dispatch_events`: ordered events belonging to a conversation and optionally a turn. Each event stores a normalized kind, role, display text, structured JSON payload, provider event/request identifier, timestamps, and sequence.

Supported event kinds are:

- `user_message`
- `assistant_message`
- `tool_call`
- `tool_result`
- `approval_request`
- `approval_decision`
- `status`
- `error`

Assistant deltas update or append to one logical assistant event rather than creating a row for every token. Tool calls and approval requests use provider identifiers for idempotent updates and exact response routing.

Existing `dispatch_run_events` rows remain untouched. The read API returns them through a legacy plain-text adapter so previous runs remain inspectable. No new conversation writes raw PTY events.

### Tauri command and event boundary

Replace terminal-specific commands with conversation semantics:

- `get_dispatch_conversation(run_id)` returns the run, turns, normalized events, and whether it can accept a prompt, interruption, approval decision, or shutdown.
- `send_dispatch_prompt(run_id, prompt)` creates a turn and starts/resumes the provider driver.
- `interrupt_dispatch_turn(run_id)` interrupts the active turn.
- `resolve_dispatch_approval(run_id, event_id, decision)` validates that the approval is still pending and forwards the decision.
- `shutdown_dispatch_conversation(run_id)` closes the logical conversation.

Keep compatibility wrappers only where existing code still needs them during the migration. Remove resize behavior and the frontend xterm dependency after all consumers use the conversation API.

The backend emits a `dispatch-event` payload containing the complete persisted normalized event. Lifecycle mutations continue to emit `data-changed` so React Query refreshes the workday rail and status controls.

All state-changing commands validate the current durable state under the database mutex, release the mutex before provider I/O, then reacquire it only for brief persistence. No database lock is held while waiting for a CLI process, stream event, or approval response.

### Approval behavior

Approval requests are durable events with a `pending`, `allowed_once`, `allowed_for_session`, `denied`, or `expired` resolution.

- **Allow once** authorizes only the exact provider request.
- **Always allow for session** uses the provider's session-scoped approval mechanism. It must not silently persist a global CLI policy.
- **Deny** rejects the exact request and records the decision before forwarding it.
- Buttons disable immediately after a decision begins and remain disabled once resolved.
- A stale, duplicate, or already-resolved decision returns an idempotent result and never targets a newer request.
- Pending approvals discovered after a Relay restart are shown as expired unless the provider protocol proves the request is still live.

Provider wording and risk details are preserved in the expandable row payload. Relay does not reclassify a provider request as safer than the provider reported.

## Frontend design

Replace `RunTerminal` with `RunChat`.

The conversation area is a chronological, scrollable feed:

- User messages are visually distinct compact bubbles.
- Assistant messages use readable prose blocks and stream in place.
- Tool calls/results use a summary row showing tool name, status, and short target; expanding reveals arguments and output using monospaced text.
- Approval rows are visually prominent, show the requested action and reason, and contain the three decision buttons while pending.
- Status and error events are quiet inline notices rather than chat bubbles.
- Legacy PTY history renders as a labeled plain-text transcript and never mounts xterm.

The composer remains fixed beneath the feed. Its controls depend on lifecycle:

- `running` or `awaiting_approval`: show **Stop turn**; prompt submission is disabled.
- `idle` or resumable `failed`: enable prompt submission.
- transitional states: disable actions and show the current transition.
- any non-shut-down state: expose **Shut down** as a separate destructive action requiring confirmation.
- `shut_down`: disable the composer and show that the conversation is closed.

Auto-scroll only when the user is already near the bottom. Expanding a tool row or reading earlier content must not yank the viewport downward when new events arrive.

## Error handling and compatibility

- Malformed provider lines become diagnostic log entries and a visible conversation error when they affect the active turn; they never panic the runtime reader.
- Unknown provider events are preserved in structured payloads for diagnostics and otherwise ignored by the chat renderer.
- Provider protocol/version failures name the affected agent and required capability.
- If an interruption times out, Relay terminates only the active provider process, marks the turn interrupted, and keeps the conversation resumable when a native session identifier exists.
- If shutdown fails to stop an OS process, Relay reports the failure and retains a non-terminal lifecycle until the process is confirmed gone.
- Old terminal conversations remain readable but cannot gain structured approvals retroactively.

## Testing

Implementation follows test-driven development.

Rust tests cover:

- provider command/protocol selection for all four agents;
- representative structured fixtures for assistant deltas, tool calls/results, approvals, completion, interruption, and malformed events;
- normalization and idempotent event persistence;
- lifecycle transitions and invalid operations;
- session-scoped approval routing and stale-decision rejection;
- stop-turn behavior preserving the open conversation;
- normal completion returning to idle;
- resume after idle, failure, and Relay restart;
- explicit shutdown disabling future prompts;
- legacy `dispatch_run_events` conversion.

Frontend tests cover:

- role-specific message rendering;
- collapsed and expanded tool rows;
- pending and resolved approval controls;
- lifecycle-driven composer, Stop turn, and Shut down behavior;
- stable streaming updates and near-bottom auto-scroll behavior;
- legacy transcript rendering without xterm.

Verification runs targeted tests first, then the full Rust test suite, frontend test suite, TypeScript typecheck, and lint. Per repository rules, Codex will not start the Vite or Tauri development server; the final handoff includes a focused manual Tauri verification checklist for the user.

## Out of scope

- A general-purpose embedded terminal.
- Screen scraping or ANSI-to-chat heuristics for new sessions.
- Changing provider-global permission policies.
- Team/cloud synchronization of live conversation state.
- Concurrent turns inside one conversation.
- Editing or deleting prior chat events.
