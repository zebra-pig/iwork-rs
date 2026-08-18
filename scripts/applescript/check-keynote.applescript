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

	-- Wait for *this* document, not for any document. `document 1` is only the
	-- right one when nothing else is open, which is not true of an app that
	-- restores its last session, and not true when two test binaries are driving
	-- the apps at once. The failure it produces is the bad kind: a complete,
	-- plausible reading of another file. Seen here as an edited deck reporting
	-- the words it had before the edit.
	set wanted to my basename(target)
	set alsoWanted to my stem(wanted)
	set doc to missing value
	repeat 60 times
		tell application id "com.apple.Keynote"
			try
				repeat with d in documents
					if (name of d) is wanted or (name of d) is alsoWanted then
						set doc to d
						exit repeat
					end if
				end repeat
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

-- The name of the document that was asked for, and the same without its
-- extension: the Finder's hide-extension flag decides which of the two the app
-- answers, and it differs between a document an app wrote itself and one this
-- crate wrote.
on basename(path)
	set AppleScript's text item delimiters to "/"
	set base to last text item of path
	set AppleScript's text item delimiters to ""
	return base
end basename

on stem(base)
	set AppleScript's text item delimiters to "."
	set pieces to text items of base
	if (count of pieces) > 1 then set pieces to items 1 thru -2 of pieces
	set base to pieces as text
	set AppleScript's text item delimiters to ""
	return base
end stem
