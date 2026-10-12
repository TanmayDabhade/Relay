//! Prompt text and JSON schemas for the planner and task-draft assistants. Pure — unit-tested
//! below without spawning the CLI.

use serde_json::{json, Value};

pub const MAX_PLANNED_TASKS: usize = 8;
/// Bounds what the user can paste in, so a runaway paste can't become a huge (costly) prompt.
pub const MAX_INPUT_CHARS: usize = 8_000;

/// What makes a prompt good for a headless coding agent, shared by both assistants so a
/// drafted task and a planned task read the same way.
const PROMPT_GUIDANCE: &str = "\
A good agent prompt is self-contained: the agent starts with no memory of this conversation. It:
- states the outcome in one or two sentences first;
- names the specific files, modules, functions, or commands involved, from what you actually \
found in the repo (never invent paths);
- lists concrete acceptance criteria, including which existing tests or checks must pass and \
which new tests to add;
- calls out constraints and conventions from the repo's CLAUDE.md / AGENTS.md / README that \
apply;
- says what is out of scope, so the agent doesn't wander.
The agent works on its own git branch and Relay commits and opens a PR when it finishes, so \
the prompt must NOT tell the agent to commit, push, or open a PR itself.";

fn clip(text: &str) -> String {
    text.trim().chars().take(MAX_INPUT_CHARS).collect()
}

pub fn draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": {
                "type": "string",
                "description": "Short imperative card title, under 70 characters."
            },
            "prompt": {
                "type": "string",
                "description": "The full self-contained prompt for the coding agent, in Markdown."
            }
        },
        "required": ["title", "prompt"],
        "additionalProperties": false
    })
}

pub fn draft_prompt(rough: &str, current_title: &str) -> String {
    let title_line = if current_title.trim().is_empty() {
        String::new()
    } else {
        format!("Current card title: {}\n", clip(current_title))
    };
    format!(
        "You are Relay's task-draft assistant. Turn the developer's rough note into a precise \
task for an autonomous coding agent that will work in this repository (your current \
directory).

First read CLAUDE.md, AGENTS.md, and README.md if they exist, then look at just enough of the \
code to make the task concrete. You are read-only: do not attempt to change anything.

{PROMPT_GUIDANCE}

Keep the developer's intent exactly. Do not expand the scope beyond what they asked for; if \
the note is ambiguous, pick the most reasonable reading and state that assumption in the \
prompt.

{title_line}Developer's rough note:
<note>
{}
</note>

Answer with the title and the prompt.",
        clip(rough)
    )
}

pub fn plan_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "summary": {
                "type": "string",
                "description": "Two or three sentences: the approach, and anything the developer should decide or know first."
            },
            "tasks": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_PLANNED_TASKS,
                "items": {
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Short imperative card title, under 70 characters."
                        },
                        "prompt": {
                            "type": "string",
                            "description": "The full self-contained prompt for the coding agent, in Markdown."
                        },
                        "rationale": {
                            "type": "string",
                            "description": "One sentence: why this task, and what it depends on."
                        }
                    },
                    "required": ["title", "prompt", "rationale"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["summary", "tasks"],
        "additionalProperties": false
    })
}

pub fn plan_prompt(goal: &str, existing_titles: &[String]) -> String {
    let existing = if existing_titles.is_empty() {
        "(none)".to_string()
    } else {
        existing_titles
            .iter()
            .take(50)
            .map(|t| format!("- {}", clip(t)))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "You are Relay's planning assistant. Break the developer's goal into an ordered list of \
1 to {MAX_PLANNED_TASKS} tasks, each small enough for one autonomous coding agent to finish \
in a single session and ship as one reviewable pull request.

First read CLAUDE.md, AGENTS.md, and README.md if they exist, then explore the code that the \
goal touches. You are read-only: do not attempt to change anything.

Planning rules:
- Order tasks so each can be built on top of the ones before it; say so in the rationale when \
a task depends on an earlier one.
- Prefer fewer, meaningful tasks over many trivial ones. A one-step goal gets one task.
- Each task must be independently reviewable and leave the project building and its tests \
passing.
- Don't duplicate work already on the board (listed below).

{PROMPT_GUIDANCE}

Cards already on this project's board:
{existing}

Developer's goal:
<goal>
{}
</goal>",
        clip(goal)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_prompt_embeds_the_note_and_title_and_forbids_self_shipping() {
        let prompt = draft_prompt("make login not break", "Login");
        assert!(prompt.contains("make login not break"));
        assert!(prompt.contains("Current card title: Login"));
        assert!(prompt.contains("must NOT tell the agent to commit"));
        assert!(!draft_prompt("x", "  ").contains("Current card title"));
    }

    #[test]
    fn plan_prompt_lists_existing_cards_and_handles_none() {
        let prompt = plan_prompt("Add dark mode", &["Fix header".to_string()]);
        assert!(prompt.contains("- Fix header"));
        assert!(prompt.contains("Add dark mode"));
        assert!(plan_prompt("g", &[]).contains("(none)"));
    }

    #[test]
    fn huge_inputs_are_clipped() {
        let prompt = draft_prompt(&"a".repeat(MAX_INPUT_CHARS * 3), "");
        assert!(prompt.len() < MAX_INPUT_CHARS * 2);
    }

    #[test]
    fn schemas_require_every_field_the_parsers_need() {
        assert_eq!(draft_schema()["required"], json!(["title", "prompt"]));
        assert_eq!(
            plan_schema()["properties"]["tasks"]["maxItems"],
            json!(MAX_PLANNED_TASKS)
        );
    }
}
