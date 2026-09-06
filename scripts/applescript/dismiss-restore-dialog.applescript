-- Click away "Do you want to try to reopen its windows again?", if it is there.
--
-- Every refusal ends with `osa_kill`, and an app killed while it had a window
-- open is offered its windows back the next time it starts:
--
--   The last time you opened Pages, it unexpectedly quit while reopening
--   windows. Do you want to try to reopen its windows again?
--
-- It is modal, it answers no Apple event, and it is in front of every document
-- opened after it — so the app stops opening documents entirely and every check
-- from then on reports a refusal. A document known to be good is then reported
-- as refused, which is this repository's worst failure mode: it was found by an
-- afternoon of measurements taken against an app that was not looking at the
-- documents at all.
--
-- Two defences, because neither is enough alone. `NSQuitAlwaysKeepsWindows` is
-- turned off for all three apps by `scripts/app-check.sh`, which stops the
-- dialog being offered; this clicks away the one that is already on screen —
-- including one left over from before that default was set.
--
-- Silent by design: the usual answer is that there is no such dialog, and this
-- runs before every single app check.

on run argv
	set target to item 1 of argv
	tell application "System Events"
		if not (exists process target) then return "not running"
		tell process target
			repeat with w in windows
				try
					if subrole of w is "AXDialog" then
						repeat with b in (every button of w)
							-- "Don't Reopen", whichever apostrophe the system
							-- draws it with.
							if name of b starts with "Don" then
								click b
								return "dismissed"
							end if
						end repeat
					end if
				end try
			end repeat
		end tell
	end tell
	return "clear"
end run
