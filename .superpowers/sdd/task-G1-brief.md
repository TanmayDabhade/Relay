# Task G1: Design system foundation — self-hosted fonts + remaining component library

Source: PLAN.md, "Ordered task breakdown" item 19 ("Design system pass: tokens.css, self-hosted fonts, retrofit all views onto it") and §7 "Frontend" (component-library note: "Component library (`src/components/ui/`) maps 1:1 to SPEC's [taxonomy]: Pill(agent/status/tag/stack variants), Card, StatTile, Modal, Button(primary/secondary), Select, TextInput, ActivityBars. Build once, reuse everywhere"). Exact visual spec: SPEC.md §8.2 (Color Tokens), §8.3 (Typography), §8.4 (Spacing), §8.5 (Component Library).

This is **G1 of a two-part final task**. G1 builds the missing foundation pieces (fonts + the remaining `src/components/ui/` components + bringing the two existing components that already deviate from spec back in line). **G2 (a separate, later task) applies these across the actual views** (Projects master-detail layout, Sessions/Timeline topbar structure, swapping raw HTML form elements for the new components). Do not do G2's work here — this task is foundation only.

## Current state (read before writing anything)

- `src/styles/tokens.css` already defines `--font-mono: "IBM Plex Mono", "Fira Code", monospace` and `--font-sans: "Inter", -apple-system, BlinkMacSystemFont, sans-serif` as font-family **names**, but **no actual font files are bundled or self-hosted anywhere in this project** — these are just CSS font-family fallback chains that currently resolve to whatever's installed on the OS (or the `-apple-system`/`monospace` generic fallback if Inter/IBM Plex Mono aren't installed). PLAN.md is explicit: "Self-host Inter + IBM Plex Mono (bundled font files, not a CDN) — consistent with 'local-first, no data leaves the machine.'" This is a real gap, not cosmetic.
- `src/components/ui/` currently has: `Pill.tsx`/`.css`, `Modal.tsx`/`.css`, `ActivityBars.tsx`/`.css`. **Missing, per SPEC §8.5 and PLAN §7's named list**: `StatTile`, `Button` (primary/secondary), `Select`, `TextInput`. (`Card` is effectively `ProjectCard`'s existing markup/CSS pattern — SPEC doesn't require a separate generic `Card` wrapper component distinct from how `ProjectCard.css` already styles a card; don't invent one unless you find a genuine second use site for it in this task's own scope, which you won't, since G1 doesn't touch views.)
- `Modal.tsx`/`Modal.css` (built in a prior task) works correctly but its exact dimensions **deviate from SPEC §8.5**: currently `max-width: 640px`, `border-radius: 10px`, backdrop `rgba(0, 0, 0, 0.35)`. SPEC §8.5 specifies: "Max-width 540px, centered, 12px radius, box-shadow `0 8px 40px rgba(0,0,0,0.13)`" and "Backdrop: `rgba(42,42,40,0.3)`". Bring `Modal.css` in line with these exact values — this is a values-only fix, the component's structure/behavior (backdrop click, Escape key, `isOpen`/`onClose`/`title`/`children` props) is correct and built already, don't touch its `.tsx` logic.
- `Pill.tsx`/`Pill.css` (built in a prior task) is functionally correct (`variant`/`tone` props, used correctly across `ProjectCard`/`SessionsView`/`SessionDetailModal`/`TimelineView`) but its styling doesn't fully match SPEC §8.5's per-variant description: "Agent pills: colored bg + border, agent name, 10–11px, 20px border-radius" (current `pill-tone-accent` — used for the `"claude"` agent pill in `ProjectCard.tsx` — has **no border**, just `background` + `color`); "Tag pills: category color, 10px, 20px border-radius" (current pills are all `font-size: var(--text-sm)` i.e. 11px uniformly, not differentiated by variant — SPEC wants tag pills specifically at 10px i.e. `--text-xs`). Bring these into alignment: add a border to accent-tone pills (a border color consistent with the existing accent palette — `--accent` or a derived subtle border shade, your call, but it must be visually present, not `transparent`), and make `.pill-tag` (if that class doesn't exist yet, since variants are currently only `agent`/`status`/`tag`/`stack` as *prop* values but CSS classes are `pill-{variant}` — check `Pill.tsx`'s className construction) render at `--text-xs` specifically for the `tag` variant, keeping other variants at their current size unless SPEC says otherwise (re-read §8.5's four bullet points yourself for the other two: "Status pills: dot + label, semantic color per status" — check whether status pills currently render a leading dot; "Stack tags: monospace, neutral gray, 4px border-radius" — `.pill-stack` already has `border-radius: 4px` and `font-family: var(--font-mono)`, this one's already correct, don't touch it).

## What to build

### 1. Self-hosted fonts

Bundle Inter and IBM Plex Mono as local font files — no CDN (`fonts.googleapis.com`, `fonts.google.com`, etc. must not appear anywhere). The cleanest, most idiomatic way to do this in a Vite project without hand-downloading binary font files is via the `@fontsource/inter` and `@fontsource/ibm-plex-mono` npm packages (these ship the actual `.woff2` files inside `node_modules` and get bundled by Vite like any other static asset when imported — no network fetch at runtime, genuinely local-first). Install them (`npm install @fontsource/inter @fontsource/ibm-plex-mono`), import the weights this app actually uses (look at what font-weights are used across the current CSS — likely just regular/400 and a semi-bold/600 for headings, e.g. `Modal.css`'s `.modal-title { font-weight: 600; }` — import only the weights you find actually referenced, not the entire family's weight range) in `src/main.tsx` or `src/styles/global.css` (your call on which, but pick one place, don't scatter font imports across files), and confirm via `npm run build` that the resulting `dist/` bundle contains the font files (not just references to an external URL).

If you have a principled objection to the `@fontsource` approach (e.g. it's unavailable, or genuinely doesn't fit), the fallback is manually sourcing the actual open-source font files (both are open-license: Inter is OFL, IBM Plex Mono is OFL) and placing them under `src/assets/fonts/` or `public/fonts/` with `@font-face` declarations in `global.css` — but prefer `@fontsource` unless you hit a concrete blocker, and document which path you took and why in your report.

### 2. `src/components/ui/StatTile.tsx` (+ CSS)

Per SPEC §8.5: "Stat tile: muted bg, monospace number, 10px label below, 8px radius." Props: something like `{ value: string | number; label: string }` (your call on exact prop names/types — e.g. whether `value` accepts a pre-formatted string like `"$12.34"` or a raw number the component formats; look at how `ProjectCard.tsx`/`SessionDetailModal.tsx` currently format numbers like cost/token counts inline and match that convention rather than inventing a third formatting approach). Styling: `background: var(--surface-muted)`, `border-radius: 8px`, the value in `var(--font-mono)`, the label below it at `var(--text-xs)` (10px). This is a small, purely presentational component — no data fetching, no logic beyond rendering.

### 3. `src/components/ui/Button.tsx` (+ CSS)

Per SPEC §8.5: "Primary button: accent bg, white text, no border, 7px radius" / "Secondary button: muted border, off-white bg, secondary text." Props: `variant: "primary" | "secondary"`, standard button props (`onClick`, `children`, `disabled`, `type`, etc. — extend `React.ButtonHTMLAttributes<HTMLButtonElement>` rather than re-declaring every native button prop by hand). Styling per variant as specified; both variants share `border-radius: 7px` and reasonable padding consistent with this design system's spacing tokens (`--space-2`/`--space-3` are reasonable candidates, your call).

### 4. `src/components/ui/Select.tsx` (+ CSS)

Per SPEC §8.5: "Select: 7px padding, 7px radius, muted border, system font." A thin wrapper around a native `<select>` (styling only — don't build a custom dropdown widget, a native `<select>` styled to spec is exactly what's asked for and is the simplest correct implementation). Props: extend `React.SelectHTMLAttributes<HTMLSelectElement>` so it's a drop-in replacement wherever a plain `<select>` is currently used (this matters for G2, which will swap `TimelineView.tsx`'s existing raw `<select>` elements for this component — don't design an incompatible prop shape that makes that swap awkward). "System font" here means don't override `font-family` to `--font-mono` — inherit/use `--font-sans` (the default UI chrome font), i.e. don't set a mono font on form controls unless SPEC says so elsewhere (it doesn't, for `Select`).

### 5. `src/components/ui/TextInput.tsx` (+ CSS)

Per SPEC §8.5: "Text input: same as select, flex-1 in dispatch row." Same padding/radius/border treatment as `Select`. Props: extend `React.InputHTMLAttributes<HTMLInputElement>`. ("flex-1 in dispatch row" refers to a specific layout context — the Agent Manager dispatch UI, which is out of scope / not built in this phase per PLAN.md's explicit deferral of Agent Manager to a later phase — so just build the input's own styling correctly; don't build a "dispatch row" layout, that doesn't exist yet.)

### 6. `Modal.css` value fixes (no `.tsx` changes)

Update to match SPEC §8.5 exactly: `max-width: 540px` (currently `640px`), `border-radius: 12px` (currently `10px`), `box-shadow: 0 8px 40px rgba(0,0,0,0.13)` (currently `0 12px 32px rgba(0,0,0,0.18)`), backdrop `background: rgba(42,42,40,0.3)` (currently `rgba(0,0,0,0.35)`). Everything else in `Modal.css`/`Modal.tsx` stays as-is — this is a values-only correction, not a redesign.

### 7. `Pill.css` alignment (minor `.tsx` change only if the current variant→className mapping can't express what's needed)

- Add a visible border to `.pill-tone-accent` (currently background+color only, no border) — SPEC's "Agent pills: colored bg + border" requirement. Pick a border color that reads correctly against the existing `--accent-subtle` background (e.g. `var(--accent)` at reduced usage, or a dedicated lighter accent-border shade if one doesn't exist in `tokens.css` yet — check first, add one to `tokens.css` only if genuinely needed, following the existing `--{color}-border` naming convention already used for green/blue/gray).
- Make tag pills render at `--text-xs` (10px) specifically, distinct from the current uniform `--text-sm` (11px) applied to all variants — check `Pill.tsx`'s className logic (`pill pill-${variant} pill-tone-${tone}`) and add a `.pill-tag { font-size: var(--text-xs); }` rule (or equivalent) that overrides the base `.pill`'s `--text-sm` specifically for the `tag` variant, without changing the size of `agent`/`status`/`stack` variants.
- Confirm (read-only check, not a change) whether status pills currently render "a dot + label" per SPEC, or just a label — report what you find; if adding a dot is a small, low-risk addition within `Pill.tsx`'s existing structure, do it (a small `<span className="pill-dot" />` colored via the existing `tone` prop is a reasonable, minimal way), but if it would require restructuring how `Pill`'s callers pass children, stop and report it as a G2-scope item instead rather than reworking call sites yourself (this task doesn't touch view files).

## Explicitly out of scope for this task (this is G2's job, later)

- No changes to any file under `src/views/` or `src/App.tsx` — this task only touches `src/components/ui/`, `src/styles/`, and font-related setup (`package.json`, `main.tsx`/`global.css` for font imports).
- No Projects master-detail layout, no `ProjectDetail` component, no Sessions/Timeline topbar restructuring, no swapping existing raw `<select>`/`<button>` elements in `TimelineView.tsx`/`SessionDetailModal.tsx` for the new `Select`/`Button` components built here — G2 does that.
- No `Card` component — not needed at this task's scope (see note above).

## Verification

- `npm run build` (`tsc -b && vite build`) must succeed with no type errors, and the build output must include the bundled font files (check `dist/assets/` for `.woff2` files after building, or equivalent evidence the fonts are actually bundled, not just referenced).
- `npm run lint` (oxlint) must be clean.
- Grep the diff for any Google Fonts / CDN font URL — must be absent.
- A dev-server smoke check (`npm run dev`, confirm no console errors, stop it) is optional here since this task doesn't touch any view that renders these new components yet (G2 wires them in) — a build success is the primary bar for this task.

## Do not commit

The user has explicitly asked that nothing gets committed to git during this session — not by you, not by the controller, at any point. Leave all your changes in the working tree, uncommitted. `git add` for your own tracking is fine; `git commit` is not.

## Report contract

Write your report to `/Users/tanmay/manageai/.superpowers/sdd/task-G1-report.md`: files changed, commands run and their output/exit codes, any deviations from this brief and why, and a DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED status. Return only a short summary plus that status to the controller — full detail goes in the report file.
