<!--
PR title: use a Conventional Commit, e.g. `feat(board): add column rename`, `fix(watcher): ...`.
Types: feat, fix, refactor, perf, test, docs, style, chore, ci, build.
Scopes (optional): parser, watcher, db, dispatch, cost, summarize, board, terminal, ui, reports, landing.
-->

## Summary

<!-- What changes, in one or two sentences. -->

## Why

<!-- The problem or motivation. Link issues with "Closes #123". For anything touching the
Architecture decisions in README.md, explain why the existing approach wasn't enough. -->

## Changes

-

## How I tested it

<!-- Commands you ran and what you checked by hand in `npm run tauri dev`.
Add a screenshot or recording for visual changes. -->

-

## Checklist

- [ ] `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` pass (from `src-tauri/`)
- [ ] `npx tsc -b`, `npm run lint`, and `node --test tests/*.test.ts` pass
- [ ] New parsing or pure logic has tests; parsers are tested against fixtures, including malformed input
- [ ] Any schema change is a **new** migration registered in `db::open` (no edits to existing migrations)
- [ ] New commands are registered in `lib.rs` and wrapped in `src/lib/tauri.ts` (no `invoke` elsewhere)
- [ ] No DB lock held across file I/O, a subprocess, network, or `.await`
- [ ] UI changes were checked by hand in `npm run tauri dev`
- [ ] Docs/comments updated where behavior changed (README, CLAUDE.md, doc comments explaining *why*)
- [ ] Changes to `landing-page/` are in a separate PR
