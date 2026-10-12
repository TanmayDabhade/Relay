-- A folder is only listed as a project once Relay has confirmed it is a git checkout whose
-- GitHub repository the user's `gh` can access (see `projects::spawn_github_verifier`).
-- Every cwd an agent ever ran in still gets a `projects` row at ingest, so sessions keep a
-- home; these columns just decide which of those rows the UI lists.
--
-- `github_repo`: `owner/name` as GitHub spells it, or NULL when unverified or not on GitHub.
-- `github_checked_at`: when the last check finished (NULL = never checked), so the verifier
-- can re-check stale rows without re-running `gh` for every project on every pass.
ALTER TABLE projects ADD COLUMN github_repo TEXT;
ALTER TABLE projects ADD COLUMN github_checked_at INTEGER;
