-- Open a .pages in Pages, read everything readable out of it, close it.
--
-- The document is handed to launch services with `open` on the command line and
-- then waited for, rather than opened with AppleScript's `open`. Told to open a
-- document, Pages opens it — and then, often enough to be useless, never
-- replies to the event: the window is there, complete, and the script that asked
-- for it is still waiting. Numbers does the same thing with an imported CSV.
-- Watching for the document to appear turns that from a hang into a poll.
--
-- The cost is that a refusal is no longer an AppleScript error with a message in
-- it: a document the app will not open simply never appears, and this script
-- reports that it did not. The alert is left on screen, so app-check.sh kills
-- the app rather than trusting it to be usable afterwards.
--
-- Every read is wrapped: a document with no body text is not a failure, and
-- neither is a table whose cells will not coerce to text.

on run argv
	set target to item 1 of argv
	set harvest to {}

	do shell script "open -g -b com.apple.Pages " & quoted form of target

	set doc to missing value
	repeat 60 times
		tell application id "com.apple.Pages"
			try
				if (count of documents) > 0 then set doc to document 1
			end try
		end tell
		if doc is not missing value then exit repeat
		delay 1
	end repeat
	if doc is missing value then error "Pages did not open " & target number 8010

	-- An Apple event gives up after two minutes by default and a big document
	-- can pass that. The shell wrapper is the real timeout.
	with timeout of 600 seconds
		tell application id "com.apple.Pages"
			try
				try
					set end of harvest to (body text of doc) as text
				end try
				repeat with t in tables of doc
					repeat with c in cells of t
						try
							set end of harvest to (value of c) as text
						end try
					end repeat
				end repeat
				repeat with s in shapes of doc
					try
						set end of harvest to (object text of s) as text
					end try
				end repeat
				repeat with i in text items of doc
					try
						set end of harvest to (object text of i) as text
					end try
				end repeat
				repeat with i in images of doc
					try
						set end of harvest to "image: " & (file name of i)
					end try
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
