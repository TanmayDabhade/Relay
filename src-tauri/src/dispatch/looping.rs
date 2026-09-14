//! "Loop until done": after each turn of a looping conversation settles, Relay sends a
//! continuation prompt on the same provider session until the agent's final message contains
//! [`LOOP_DONE_MARKER`] or the run's iteration cap is reached. Everything here is pure so the
//! stopping rules are unit-tested without spawning a provider.

/// Upper bound accepted from the UI. The loop spends real money unattended, so a typo like
/// 500 must be rejected rather than silently honored.
pub const MAX_LOOP_ITERATIONS: i64 = 50;

/// Deliberately not a natural phrase ("done", "complete") that an agent would write while
/// summarizing partial progress.
pub const LOOP_DONE_MARKER: &str = "RELAY_LOOP_DONE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopDecision {
    /// The run is not looping, or the turn didn't finish cleanly — leave it for the user.
    Stop,
    /// The agent declared the task finished.
    Done,
    /// The cap was reached without a done signal.
    CapReached { max: i64 },
    /// Send continuation number `iteration` (1-based) of `max`.
    Continue { iteration: i64, max: i64 },
}

/// Decides what follows a settled turn. Only a `completed` turn on an `idle` conversation
/// continues: an interrupted turn means the user pressed stop, a failed one (e.g. the CLI's
/// own auth failure) would just fail again in a tight loop, and a `shut_down` or
/// `awaiting_approval` conversation needs a human.
pub fn decide(
    turn_status: &str,
    run_status: &str,
    loop_max_iterations: Option<i64>,
    loop_iterations: i64,
    last_assistant_message: Option<&str>,
) -> LoopDecision {
    let Some(max) = loop_max_iterations else {
        return LoopDecision::Stop;
    };
    if turn_status != "completed" || run_status != "idle" {
        return LoopDecision::Stop;
    }
    if last_assistant_message.is_some_and(declares_done) {
        return LoopDecision::Done;
    }
    if loop_iterations >= max {
        return LoopDecision::CapReached { max };
    }
    LoopDecision::Continue {
        iteration: loop_iterations + 1,
        max,
    }
}

fn declares_done(message: &str) -> bool {
    message.contains(LOOP_DONE_MARKER)
}

pub fn validate_max_iterations(value: Option<i64>) -> Result<Option<i64>, String> {
    match value {
        None => Ok(None),
        Some(max) if (1..=MAX_LOOP_ITERATIONS).contains(&max) => Ok(Some(max)),
        Some(max) => Err(format!(
            "loop iterations must be between 1 and {MAX_LOOP_ITERATIONS}, got {max}"
        )),
    }
}

/// The first prompt of a looping run: the user's instructions plus the stopping contract.
pub fn initial_prompt(task_prompt: &str) -> String {
    format!(
        "{task_prompt}\n\n---\nRelay is running this task in a loop: after each of your turns \
         it will ask you to continue. When the task is fully complete and verified, end your \
         final message with {LOOP_DONE_MARKER} on its own line. Do not write that marker \
         before then."
    )
}

pub fn continuation_prompt(iteration: i64, max: i64) -> String {
    format!(
        "Continue working on the task (loop iteration {iteration} of {max}). Review what is \
         left, keep going, and verify your work. If everything is complete and verified, end \
         your final message with {LOOP_DONE_MARKER} on its own line."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_looping_runs_never_continue() {
        assert_eq!(
            decide("completed", "idle", None, 0, Some("hi")),
            LoopDecision::Stop
        );
    }

    #[test]
    fn completed_turn_without_marker_continues_with_the_next_iteration() {
        assert_eq!(
            decide("completed", "idle", Some(3), 0, Some("made progress")),
            LoopDecision::Continue {
                iteration: 1,
                max: 3
            }
        );
        assert_eq!(
            decide("completed", "idle", Some(3), 2, None),
            LoopDecision::Continue {
                iteration: 3,
                max: 3
            }
        );
    }

    #[test]
    fn marker_in_final_message_ends_the_loop_even_at_the_cap() {
        let message = format!("All tests pass.\n{LOOP_DONE_MARKER}");
        assert_eq!(
            decide("completed", "idle", Some(3), 0, Some(&message)),
            LoopDecision::Done
        );
        assert_eq!(
            decide("completed", "idle", Some(3), 3, Some(&message)),
            LoopDecision::Done
        );
    }

    #[test]
    fn cap_stops_the_loop() {
        assert_eq!(
            decide("completed", "idle", Some(3), 3, Some("still going")),
            LoopDecision::CapReached { max: 3 }
        );
    }

    #[test]
    fn interrupted_failed_or_shut_down_turns_do_not_continue() {
        for (turn, run) in [
            ("interrupted", "idle"),
            ("failed", "failed"),
            ("completed", "shut_down"),
            ("completed", "awaiting_approval"),
        ] {
            assert_eq!(
                decide(turn, run, Some(5), 0, Some("x")),
                LoopDecision::Stop,
                "{turn}/{run}"
            );
        }
    }

    #[test]
    fn iteration_cap_is_validated() {
        assert_eq!(validate_max_iterations(None), Ok(None));
        assert_eq!(validate_max_iterations(Some(1)), Ok(Some(1)));
        assert_eq!(
            validate_max_iterations(Some(MAX_LOOP_ITERATIONS)),
            Ok(Some(MAX_LOOP_ITERATIONS))
        );
        assert!(validate_max_iterations(Some(0)).is_err());
        assert!(validate_max_iterations(Some(MAX_LOOP_ITERATIONS + 1)).is_err());
    }

    #[test]
    fn prompts_carry_the_stopping_contract() {
        assert!(initial_prompt("Fix the bug").starts_with("Fix the bug"));
        assert!(initial_prompt("Fix the bug").contains(LOOP_DONE_MARKER));
        assert!(continuation_prompt(2, 5).contains("2 of 5"));
        assert!(continuation_prompt(2, 5).contains(LOOP_DONE_MARKER));
    }
}
