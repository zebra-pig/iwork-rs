-- Open a .numbers in Numbers, read every cell of every table of every sheet,
-- close it.
--
-- Opened through launch services and then waited for, for the reason
-- check-pages.applescript sets out: the app opens the document and does not
-- always answer the event that asked it to.
--
-- Cells are fetched a whole table at a time — `formatted value of every cell` is
-- one Apple event, where a loop over 2400 cells would be 2400 of them and take
-- minutes. The formatted value is what the app displays, which is what a caller
-- checking "did my edit land" wants to compare against.

on run argv
	set target to item 1 of argv
	set harvest to {}

	do shell script "open -g -b com.apple.Numbers " & quoted form of target

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
		tell application id "com.apple.Numbers"
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
	if doc is missing value then error "Numbers did not open " & target number 8010

	with timeout of 600 seconds
		tell application id "com.apple.Numbers"
			try
				repeat with s in sheets of doc
					set end of harvest to "sheet: " & (name of s)
					repeat with t in tables of s
						set end of harvest to "table: " & (name of t)
						-- **A categorised table will not hand over its cells.**
						-- `formatted value of every cell` on a table with
						-- categories switched on answers -10000, "The cell
						-- formatted value cannot be retrieved" — the app has
						-- the document open and simply will not enumerate a
						-- grid that has group and summary rows in it. That is
						-- not a document this app refuses, which is the
						-- question this script exists to answer, so the table
						-- is noted and skipped rather than failing the check.
						try
							set cellText to formatted value of every cell of t
							repeat with v in cellText
								try
									set end of harvest to v as text
								on error
									set end of harvest to ""
								end try
							end repeat
						on error cellError number cellCode
							set end of harvest to "cells unavailable (" & cellCode & "): " & cellError
						end try
					end repeat
					-- Text on a sheet that is not in a table. A Numbers sheet
					-- holds shapes and text items like any other iWork
					-- container, and the templates put addresses, notes and —
					-- the only hyperlinks in the whole install — in them. A
					-- reader that stops at the tables cannot see an edit to any
					-- of that, and answers "not found" about a document that
					-- contains it.
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
