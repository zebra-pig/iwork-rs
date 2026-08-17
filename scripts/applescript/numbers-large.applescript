-- A table big enough to stop fitting in one tile.
--
-- Built by importing a CSV rather than by setting cells: a cell written over
-- Apple events costs about 150 ms, so the 2400 cells here would take six
-- minutes one at a time, and Numbers imports the same data in about one second.
-- The row count is the point — Numbers stores a table's cells in tiles, and a
-- table that fits in a single tile never exercises the boundary between two.
--
-- The import is started by `open` on the *command line*, not by AppleScript's
-- `open`. Told to open a CSV, Numbers performs the import and then, often
-- enough to be useless, never replies to the event — the document is there, on
-- screen, complete, and the script that asked for it is still waiting. Handing
-- the job to launch services and then watching for the document to appear turns
-- that from a hang into a poll.
--
-- Two formulas are added afterwards, each over a whole imported column, so the
-- document also holds a calculation that spans the tiles.

on run argv
	set target to item 1 of argv
	set source to item 2 of argv
	set wanted to item 3 of argv

	do shell script "open -g -b com.apple.Numbers " & quoted form of source

	set doc to missing value
	repeat 120 times
		tell application id "com.apple.Numbers"
			try
				set doc to document wanted
			end try
		end tell
		if doc is not missing value then exit repeat
		delay 1
	end repeat
	if doc is missing value then error "Numbers never opened " & source number 8001

	with timeout of 900 seconds
		tell application id "com.apple.Numbers"
			try
				-- The rows arrive after the document does.
				set rowTotal to 0
				repeat 60 times
					try
						set rowTotal to row count of table 1 of sheet 1 of doc
					end try
					if rowTotal > 1 then exit repeat
					delay 1
				end repeat
				if rowTotal ≤ 1 then error "the CSV import produced no rows" number 8002

				tell sheet 1 of doc
					set name to "Import"
					tell table 1
						set name to "Zeilen"
						set column count to (column count) + 1
						set value of cell 9 of row 1 to "Prüfsumme"
						set value of cell 9 of row 2 to "=SUM(C2:C" & rowTotal & ")"
						set value of cell 9 of row 3 to "=AVERAGE(E2:E" & rowTotal & ")"
						set answer to "rows=" & rowTotal & " sum=" & ¬
							(value of cell 9 of row 2 as text)
					end tell
				end tell
				save doc in POSIX file target
			on error message number code
				try
					close doc saving no
				end try
				error message number code
			end try
			close doc saving no
		end tell
	end timeout
	return answer
end run
