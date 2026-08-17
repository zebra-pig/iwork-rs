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

	set doc to missing value
	repeat 60 times
		tell application id "com.apple.Numbers"
			try
				if (count of documents) > 0 then set doc to document 1
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
