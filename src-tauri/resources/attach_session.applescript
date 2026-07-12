-- Invoked as `osascript - <project_path> <resume_session_id_or_empty>` with this script
-- piped to stdin. Prompt text is expected to already be on the clipboard (see
-- terminal::attach_or_launch) — pasted with Cmd+V rather than typed character-by-character,
-- since Claude Code's interactive input submits on a bare Return, and `keystroke`ing a
-- multi-line prompt would submit it early at the first newline.
--
-- Returns one of: "attached_existing_tab" | "resumed_in_new_window" | "started_new_window".

on run argv
	set targetPath to item 1 of argv
	set resumeId to item 2 of argv

	set foundTab to my findClaudeTab(targetPath)
	if foundTab is not missing value then
		my pasteIntoFrontmost()
		return "attached_existing_tab"
	end if

	my openNewWindowAndRun(targetPath, resumeId)
	delay 2
	my pasteIntoFrontmost()

	if resumeId is not "" then
		return "resumed_in_new_window"
	else
		return "started_new_window"
	end if
end run

-- Scans every open Terminal.app tab for one whose foreground process is `claude` with a
-- cwd matching targetPath. Each tab is checked in its own `try` so one tab we can't
-- introspect (permission denied, process just exited, etc.) doesn't abort the whole scan.
-- On a match, brings that tab's window to front and selects the tab before returning it.
on findClaudeTab(targetPath)
	tell application "Terminal"
		repeat with w in windows
			repeat with t in tabs of w
				try
					set tabTty to tty of t
					set ttyName to do shell script "basename " & quoted form of tabTty
					set psOut to do shell script "ps -t " & ttyName & " -o pid=,comm= | grep -m1 -w claude"
					set pidStr to word 1 of psOut
					set cwdOut to do shell script "lsof -a -p " & pidStr & " -d cwd -Fn 2>/dev/null | tail -1 | cut -c2-"
					if cwdOut is equal to targetPath then
						set index of w to 1
						set selected tab of w to t
						activate
						return t
					end if
				end try
			end repeat
		end repeat
	end tell
	return missing value
end findClaudeTab

on openNewWindowAndRun(targetPath, resumeId)
	set claudeCmd to "claude"
	if resumeId is not "" then
		set claudeCmd to "claude --resume " & quoted form of resumeId
	end if
	set shellCmd to "cd " & quoted form of targetPath & " && " & claudeCmd

	tell application "Terminal"
		activate
		do script shellCmd
	end tell
end openNewWindowAndRun

on pasteIntoFrontmost()
	tell application "Terminal" to activate
	delay 0.3
	tell application "System Events" to keystroke "v" using command down
end pasteIntoFrontmost
