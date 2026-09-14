-- "Loop until done" for Relay-owned conversations. NULL loop_max_iterations means the run
-- is not looping; loop_iterations counts automatic continuation turns Relay has sent.
ALTER TABLE dispatch_runs ADD COLUMN loop_max_iterations INTEGER;
ALTER TABLE dispatch_runs ADD COLUMN loop_iterations INTEGER NOT NULL DEFAULT 0;
