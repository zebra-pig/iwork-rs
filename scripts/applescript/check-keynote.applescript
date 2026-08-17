-- Open a .key in Keynote, read every slide's title, body and presenter notes,
-- close it.
--
-- Opened through launch services and then waited for, for the reason
-- check-pages.applescript sets out: the app opens the document and does not
-- always answer the event that asked it to.
--
-- A slide whose layout has no title or no body answers `default title item` with
-- an error rather than with an empty shape, so each read stands on its own.
-- Presenter notes are read too: they are text the app keeps well away from the
-- slide, and a reader that finds only what is visible would miss them.

on run argv
	set target to item 1 of argv
	set harvest to {}

	do shell script "open -g -b com.apple.Keynote " & quoted form of target

	set doc to missing value
	repeat 60 times
		tell application id "com.apple.Keynote"
			try
				if (count of documents) > 0 then set doc to document 1
			end try
		end tell
		if doc is not missing value then exit repeat
		delay 1
	end repeat
	if doc is missing value then error "Keynote did not open " & target number 8010

	with timeout of 600 seconds
		tell application id "com.apple.Keynote"
			try
				repeat with s in slides of doc
					set end of harvest to "slide " & (slide number of s) & ¬
						" (" & (name of base layout of s) & ")" & ¬
						" skipped=" & (skipped of s as text)
					try
						set end of harvest to (object text of default title item of s) as text
					end try
					try
						set end of harvest to (object text of default body item of s) as text
					end try
					try
						set end of harvest to (presenter notes of s) as text
					end try
					repeat with i in text items of s
						try
							set end of harvest to (object text of i) as text
						end try
					end repeat
					repeat with i in shapes of s
						try
							set end of harvest to (object text of i) as text
						end try
					end repeat
				end repeat
			on error message number code
				try
					close doc saving no
				end try
				error message number code
			end try
			close doc saving no
		end tell
	end timeout

	set AppleScript's text item delimiters to linefeed
	set answer to harvest as text
	set AppleScript's text item delimiters to ""
	return answer
end run
