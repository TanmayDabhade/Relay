# Streamed Agent Chat and Interface Size

**Date:** 2026-08-27  
**Status:** Approved in chat; pending written-spec review

## Purpose

Make Relay-owned agent conversations read like a polished ChatGPT or Claude conversation rather than a structured event debugger. Assistant prose is the primary transcript. Tool work becomes one quiet, continuously updated activity line per turn, while approvals and failures remain prominent because they require attention.

Replace viewport-driven font scaling with a persistent Small, Medium, or Large interface-size preference that scales the entire application consistently.

## User-visible contract

### Conversation presentation

- User prompts remain compact, visually distinct messages.
- Assistant prose appears as normal readable messages and updates in place while the provider streams it.
- Relay never creates one visible message per token or repeats cumulative provider snapshots.
- Ordinary tool calls, results, reasoning records, and non-fatal provider diagnostics do not render as separate bordered rows.
- Each turn owns at most one muted activity line. While work is active, the line changes to the latest meaningful action, such as `Reading README.md…` or `Running tests…`.
- When the turn settles, the same line remains as a muted summary such as `Used 7 tools`. It is clickable and expands to show the chronological tool names, states, arguments, outputs, and non-fatal diagnostics.
- Tool details are available for trust and debugging but never compete visually with the assistant response.
- Pending approvals remain prominent action cards with **Allow once**, **Always allow for session**, and **Deny**.
- Provider or turn failures that prevent completion remain prominent errors. Ordinary stderr warnings move into the expanded activity details and do not appear as standalone transcript banners.
- Historical conversations created by the current implementation are compacted at read time so their separate tool-call and tool-result rows also render as one activity line.

### Streaming behavior

- A logical assistant message keeps the same database id and transcript position while its content grows.
- A logical tool activity keeps the same database id and sequence while its state changes from running to completed or failed.
- The backend emits the complete current event after every update. The frontend replaces the cached event with the same id instead of appending a duplicate.
- Auto-scroll follows updates only while the reader is near the bottom. Reading earlier messages or expanded activity never gets interrupted by a forced jump.
- If a provider supplies only a final message rather than deltas, Relay displays it when received; Relay does not simulate streaming.

### Interface size

- Settings becomes a dedicated sidebar destination.
- Settings contains an **Interface size** control with **Small**, **Medium**, and **Large** choices.
- The choice scales typography, spacing, controls, navigation, and panels together because the existing design tokens use `rem` units.
- The fixed root sizes are 14 px for Small, 16 px for Medium, and 18 px for Large. Medium is the default.
- Window width never changes font size.
- The preference applies immediately, persists locally, and is restored before the first React render to avoid a flash at the wrong size.
- The existing operating-system light/dark preference remains unchanged.

## Architecture

### Normalized event updates

Extend the in-memory normalized event contract with an update mode that tells persistence whether incoming content replaces the current snapshot or appends a true delta. Provider normalizers are responsible for producing one stable `provider_event_id` per logical assistant message, tool activity, or approval.

Provider behavior:

- **Claude Code:** message ids identify assistant prose snapshots; tool-use block ids identify tool activity. Tool results update the matching tool-use event.
- **Codex CLI:** item ids identify agent messages and tool activity. `item.updated` is normalized as an in-place update rather than ignored, and `item.completed` settles the same event.
- **Gemini CLI:** message or tool ids are used when available. Events without a stable provider id append once and do not claim streaming support.
- **Cursor Agent:** message and tool ids update existing events when present; unidentifiable events append defensively.

The normalizer must distinguish cumulative snapshots from true deltas so repeated provider content cannot produce duplicated prose.

### Durable event upsert

Replace append-only persistence for identified provider events with `upsert_dispatch_event`:

1. Look for an existing event in the same run and turn with the provider event id and logical channel.
2. Update its kind, role, content, payload, and state according to the normalized update mode.
3. Preserve its database id, sequence, and original creation time.
4. Append only when no logical event exists or when the provider supplies no stable identifier.
5. Return the complete persisted event for emission to the frontend.

No destructive migration of existing transcript history is required. The current non-unique provider-id index remains useful for lookup, and historical duplicates are handled by the frontend projection. Avoid adding a unique constraint that could fail on existing duplicate tool rows.

Approval requests continue using their exact provider request identifier. Updating assistant or tool events must never coalesce with approval events that happen to reuse an identifier.

### Frontend conversation projection

Add a pure conversation projector between durable events and React rendering. It produces:

- user message blocks;
- assistant message blocks;
- one activity block per turn;
- approval blocks;
- fatal error blocks.

The activity block groups `tool_call`, `tool_result`, reasoning, status diagnostics, and warning events by `turn_id`. Provider ids pair historical call/result duplicates; unpaired events remain available in chronological details. Its collapsed label is derived from the newest active item while the turn runs and from aggregate counts after completion.

`RunChat` renders the projected blocks rather than mapping raw database events directly. Live event handling becomes an id-based merge: replace an existing event with the same id, otherwise insert it by sequence. This keeps React keys stable during streaming.

The collapsed activity control is deliberately unboxed: muted text, a small disclosure chevron, and a subtle animated ellipsis only while active. Expanded details use a quiet indented list with optional monospaced payload/output sections. Approval and fatal-error styling remain visually distinct.

### Settings and preference storage

Add a focused frontend preference module with:

- the `InterfaceSize` type (`small | medium | large`);
- validation and fallback to `medium`;
- local persistence under a versioned Relay preference key;
- one function that applies `data-interface-size` to `document.documentElement`.

Initialize the attribute in `main.tsx` before mounting React. A dedicated `SettingsView` reads and changes the preference through this module. No backend command or database migration is needed because this is a device-local presentation preference with no cross-device semantics.

Replace the viewport `clamp()` on `html` with fixed attribute selectors:

```css
html { font-size: 16px; }
html[data-interface-size="small"] { font-size: 14px; }
html[data-interface-size="large"] { font-size: 18px; }
```

The Settings control uses native radio semantics and shows a small live preview. Keyboard focus and reduced-motion behavior are preserved.

## Data flow

```text
provider structured line
  -> provider normalizer (stable id + append/replace mode)
  -> SQLite event upsert
  -> complete persisted event emitted
  -> React Query cache merges by database id
  -> conversation projector groups one activity block per turn
  -> assistant prose and muted activity render in place
```

On reload, `get_dispatch_conversation` returns the durable current event snapshots. The same projector reconstructs the transcript, so live and historical rendering use one path.

## Error handling

- Malformed structured lines remain backend diagnostics. A line that prevents the turn from completing becomes a fatal error block; a recoverable warning joins that turn’s activity details.
- An update without a matching prior event safely appends a new event.
- An event with an unknown update mode is rejected in normalization tests rather than guessed in persistence.
- Missing or malformed interface-size preferences fall back to Medium and overwrite no unrelated local storage.
- Storage write failures do not prevent the setting from applying for the current app session.
- Existing legacy PTY transcripts retain their labeled plain-text fallback.

## Testing

Implementation follows test-driven development.

Rust tests cover:

- provider normalization of assistant snapshots and deltas;
- Codex `item.updated` followed by `item.completed`;
- tool calls and results updating one durable event;
- assistant updates preserving id and sequence;
- append fallback when provider ids are absent;
- separation of tool, assistant, and approval channels when ids collide;
- malformed events and database failures.

Frontend tests cover pure behavior without mocking React:

- live cache replacement by event id;
- historical call/result pairing;
- one activity block per turn;
- active and completed activity labels;
- approvals and fatal errors staying outside activity details;
- Small, Medium, and Large preference validation and restoration.

Verification runs targeted tests first, then the full Rust test suite, TypeScript typecheck, lint, and whitespace checks. Repository instructions prohibit Codex from starting the Tauri development server, so handoff includes a manual check for real provider streaming, near-bottom scroll behavior, activity expansion, approvals, and all three interface sizes.

## Out of scope

- Hiding approval requests or fatal errors.
- Deleting raw tool data from SQLite.
- Simulated typing for providers without streaming events.
- Cloud synchronization of appearance preferences.
- Theme selection or custom numeric font sizes.
- Reworking Markdown rendering or adding syntax highlighting.
