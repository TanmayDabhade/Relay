pub mod claude_jsonl;
pub mod session_builder;
pub mod transcript;

pub use claude_jsonl::parse_line;
pub use session_builder::{ingest_record, IngestOutcome};
pub use transcript::extract_excerpts;
// `TranscriptExcerpts` isn't referenced by name yet (its `last_assistant_text` field is for
// Task C2's summarization pipeline, not this task's tag classification) - re-exported here so
// that future consumer can reach it via `parser::TranscriptExcerpts` without reaching into the
// submodule directly.
#[allow(unused_imports)]
pub use transcript::TranscriptExcerpts;
