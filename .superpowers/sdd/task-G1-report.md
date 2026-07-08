# Task G1 Report: Design system foundation — self-hosted fonts + remaining component library

## Status: DONE

## What I implemented

### 1. Self-hosted fonts
- Installed `@fontsource/inter` (^5.2.8) and `@fontsource/ibm-plex-mono` (^5.2.7) via npm (added to `package.json`/`package-lock.json`).
- Imported only the weights actually referenced across the existing CSS (checked with `grep -rn "font-weight" src/` and cross-referenced which selectors also set `font-family: var(--font-mono)` vs default `--font-sans`):
  - Inter 400 (body default), 500 (`Sidebar.css` `.sidebar-nav-item.active`), 600 (`Modal.css` `.modal-title`, plus `ProjectCard.css`/`SessionDetailModal.css`/`SessionsView.css`/`TimelineView.css` headings at 600).
  - IBM Plex Mono 400 (default, used wherever `--font-mono` appears without an explicit weight) and 600 (`Sidebar.css` `.sidebar-brand` combines `font-family: var(--font-mono)` with `font-weight: 600`).
- Imports live in one place, `src/main.tsx`, as five explicit `import '@fontsource/<family>/<weight>.css'` statements immediately before the existing `tokens.css`/`global.css` imports. Picked `main.tsx` (JS imports of the `@fontsource` CSS files) over a bare `@import` in `global.css` because it's the documented/idiomatic `@fontsource` pattern and avoids relying on Vite's bare-specifier resolution inside `@import` at-rules.
- No CDN — no `fonts.googleapis.com`/`fonts.google.com`/`fonts.gstatic.com` reference anywhere (`grep -rn` across `src/` and `package.json` returns nothing).

### 2. `src/components/ui/StatTile.tsx` + `.css`
Props: `{ value: string | number; label: string }` — matches how `ProjectCard.tsx`/`SessionDetailModal.tsx` already format cost/duration as pre-formatted strings inline (e.g. `` `$${project.total_cost_usd.toFixed(2)}` ``) rather than having the component do numeric formatting itself, so callers keep full control of formatting (currency symbols, units, etc.) and just hand `StatTile` a ready string.
Styling: `background: var(--surface-muted)`, `border-radius: 8px`, value in `var(--font-mono)` at `--text-md`, label at `--text-xs` below.

### 3. `src/components/ui/Button.tsx` + `.css`
`variant: "primary" | "secondary"` (default `"primary"`), extends `React.ButtonHTMLAttributes<HTMLButtonElement>`, spreads all native props through, merges an optional caller-supplied `className`. Both variants: `border-radius: 7px`, `padding: var(--space-2) var(--space-3)`. Primary: `background: var(--accent)`, white text, no border. Secondary: `background: var(--surface-raised)`, `border: 1px solid var(--border-strong)`, `color: var(--text-secondary)`. Added `:disabled` and `:hover` states as reasonable polish (hover uses `filter: brightness()` rather than inventing a new darker-accent token, since `--border-selected` turned out to be the exact same hex as `--accent` — using it for hover would have produced no visible change).

### 4. `src/components/ui/Select.tsx` + `.css`
Thin styled wrapper around native `<select>`. Extends `React.SelectHTMLAttributes<HTMLSelectElement>` fully (no prop re-declaration), spreads `...rest` and forwards `children` (so `<option>` children work exactly as with a raw `<select>` — this is what makes it a genuine drop-in for `TimelineView.tsx`'s existing raw `<select>` elements when G2 does that swap). Styling: 7px padding, 7px radius, `border: 1px solid var(--border-strong)`, `font-family: var(--font-sans)` (explicitly NOT mono, per SPEC's "system font" wording), plus a focus outline using `--accent`.

### 5. `src/components/ui/TextInput.tsx` + `.css`
Same treatment as `Select`: extends `React.InputHTMLAttributes<HTMLInputElement>`, spreads `...rest`, merges `className`. Identical padding/radius/border/font treatment to `Select` per SPEC's "same as select" wording. No `flex-1`/dispatch-row layout built (out of scope, Agent Manager not built yet).

### 6. `Modal.css` value fixes (`.tsx` untouched)
- `max-width: 640px` → `540px`
- `border-radius: 10px` → `12px`
- `box-shadow: 0 12px 32px rgba(0,0,0,0.18)` → `0 8px 40px rgba(0,0,0,0.13)`
- Backdrop `rgba(0,0,0,0.35)` → `rgba(42,42,40,0.3)`

All four exact values now match SPEC §8.5 verbatim. `Modal.tsx` was not opened for editing at all — confirmed via `git diff` that only `Modal.css` changed.

### 7. `Pill.css`/`Pill.tsx` alignment
- **Accent border**: added a new token `--accent-border: #e3b6a2` to `tokens.css` (lighter/desaturated tint of `--accent: #c96a3a`, following the same relationship pattern as `--green`/`--green-border` and `--blue`/`--blue-border`), and set `.pill-tone-accent { border-color: var(--accent-border); }` so agent pills (`ProjectCard.tsx`'s `<Pill variant="agent" tone="accent">`) now render with a visible border instead of `border: transparent` (inherited from base `.pill`).
- **Tag font size**: added `.pill-tag { font-size: var(--text-xs); }`. Since CSS cascade order puts this rule after `.pill`'s base `font-size: var(--text-sm)` but the className construction is `pill pill-${variant} pill-tone-${tone}` (variant class always present, e.g. `pill-tag` for `variant="tag"`), this correctly overrides to 10px for tag pills only, leaving `agent`/`status`/`stack` at their existing sizes (`--text-sm` for agent/status, and `pill-stack` already had no explicit font-size override of its own beyond the base — left untouched, confirmed unaffected since `.pill-stack`'s selector specificity is separate from `.pill-tag` and I didn't touch it).
- **Status dot — read-only check + fix**: I checked whether status pills currently render "a dot + label" per SPEC's "Status pills: dot + label, semantic color per status" bullet. **Finding: they did not** — `Pill.tsx` only rendered `{children}` (the status label text) inside the `<span>`, with no dot element, for all three call sites (`SessionsView.tsx:26`, `TimelineView.tsx:84`, `SessionDetailModal.tsx:73`, all `<Pill variant="status" tone={...}>`). Per the brief's instruction, since fixing this was a small, low-risk, self-contained change inside `Pill.tsx`'s own render body (no call-site changes needed — callers already just pass `tone` + text children, which is exactly what a dot-prepending change needs), I implemented it: `Pill.tsx` now renders `{variant === "status" && <span className="pill-dot" />}` before `{children}`, and `Pill.css` adds a `.pill-dot` rule (6px circle, `background: currentColor`, `flex-shrink: 0` — it inherits the tone's color automatically via `currentColor` since `.pill-tone-*` already sets `color`, and gets spacing for free from `.pill`'s existing `gap: var(--space-1)` flex layout). Verified via `git diff` that no file under `src/views/` changed as part of this — the three call sites need zero modification since they already only pass `tone` + text children.
- **Stack tags**: confirmed already correct per brief (`--font-mono`, `border-radius: 4px`) — left untouched, no diff.

## Verification

- `npm run build` (`tsc -b && vite build`): **exit 0**, no type errors. Build output includes 31 `.woff2` font files (`dist/assets/inter-*.woff2`, `dist/assets/ibm-plex-mono-*.woff2`, plus `.woff` fallbacks and per-subset variants — Latin, Latin-ext, Cyrillic, Greek, Vietnamese, etc., which `@fontsource` ships by default per weight). Confirmed with `ls dist/assets/*.woff2 | wc -l` → `31`, and `grep -o "https://fonts[^)]*" dist/assets/index-*.css` → no match (fonts are referenced via local hashed asset URLs only, e.g. `url(/assets/inter-latin-400-normal-*.woff2)`).
- `npm run lint` (oxlint): **exit 0**, clean, no warnings/errors.
- `grep -rn "fonts.googleapis.com\|fonts.google.com\|fonts.gstatic.com" src/ package.json`: no matches.

## Files changed

New:
- `src/components/ui/StatTile.tsx`, `src/components/ui/StatTile.css`
- `src/components/ui/Button.tsx`, `src/components/ui/Button.css`
- `src/components/ui/Select.tsx`, `src/components/ui/Select.css`
- `src/components/ui/TextInput.tsx`, `src/components/ui/TextInput.css`

Modified:
- `src/main.tsx` (font imports)
- `src/styles/tokens.css` (added `--accent-border`)
- `src/components/ui/Modal.css` (four value fixes)
- `src/components/ui/Pill.css` (`.pill-tag`, `.pill-dot`, `.pill-tone-accent` border)
- `src/components/ui/Pill.tsx` (status dot render)
- `package.json`, `package-lock.json` (new deps: `@fontsource/inter`, `@fontsource/ibm-plex-mono`)

## Self-review

- **Zero view-file changes**: confirmed via `git diff --stat -- src/views src/App.tsx` — I did not touch `src/App.tsx`. Note: `git status`/`git diff` show *pre-existing* unstaged/uncommitted changes to `src/views/ProjectCard.css`, `src/views/ProjectsView.css`, `src/views/ProjectsView.tsx` and various `src-tauri/` files that were already present in the working tree before I started (leftover uncommitted state from prior Phase 1/2 work, per the task context) — I verified these are not mine by checking I never opened/edited those paths in this session, and their diffs are unrelated to fonts/components/tokens.
- **No `Card` component invented**: confirmed, did not create one.
- **No Agent Manager dispatch-row layout built**: confirmed, `TextInput`/`Select` are standalone styled components only.
- **Select/TextInput drop-in compatibility**: both extend the full native HTML attributes interface and spread `...rest`, so existing raw `<select>`/`<input>` usages can swap in with just a prop-name-compatible replacement (G2's job) without prop-shape friction.
- **Modal.tsx untouched**: confirmed via diff — only `Modal.css` has changes.
- **Pill status-dot**: implemented (see above) since it required zero call-site changes, per the brief's explicit conditional instruction.

## Deviations / concerns

- None from the brief's explicit instructions. One judgment call: chose `main.tsx` JS imports over a `global.css` `@import` for font loading — brief said either was acceptable ("your call on which, but pick one place"). Documented reasoning above.
- One minor judgment call: `--accent-border: #e3b6a2` is a new token value I chose (lighter tint of `--accent`) since no existing accent-border token existed; brief explicitly permitted adding one "if genuinely needed."
- `@fontsource` ships multiple Unicode-range subsets (Latin, Latin-ext, Cyrillic, Greek, Vietnamese) per weight by default, which is why the bundled font-file count is higher than "2 families × ~3 weights" might suggest (31 files, not ~5) — this is standard `@fontsource` behavior (the browser only downloads the subset(s) it needs via `unicode-range`, so this isn't wasted bandwidth for the actual user, just more files sitting in `dist/`). Did not attempt to restrict to Latin-only subsets since the brief didn't ask for that level of optimization and it's a reasonable, standard default.
