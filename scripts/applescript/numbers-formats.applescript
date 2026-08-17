-- Every cell data format Numbers will set from a script, and every shape of
-- merge, in one document.
--
-- The point of this fixture is the two things a cell carries *besides* its
-- value. A format is an interned `TSK.FormatStructArchive` in the table's
-- format list, and the cell record holds a key into it; a control (checkbox,
-- rating, slider, stepper, pop-up menu) is a format too, plus a
-- `TST.CellSpecArchive` in a second list. Neither is inferable from the value,
-- so a reader that reports 0.25 for a percentage cell disagrees with the app.
--
-- Merges are stored somewhere else again, and which of the three possible
-- places a document uses depends on how the merge was made. Three shapes here —
-- across, down, and a rectangle — so the answer cannot be a coincidence of one.
--
-- Every format is applied inside a `try`: the dictionary lists formats the app
-- may refuse for a given value, and one refusal should cost one cell rather
-- than the whole document. What was actually applied comes back in the result,
-- and the app is asked to read it back — it is the oracle for the mapping from
-- `format_type` to a name.

on run argv
	set target to item 1 of argv
	set applied to {}

	-- label, value, format name. The value is written first: Numbers picks a
	-- storage type from the value and then a format is a view on it.
	set plan to {¬
		{"Automatisch", 1234.5, "automatic"}, ¬
		{"Zahl", 1234.5, "number"}, ¬
		{"Währung", 19.99, "currency"}, ¬
		{"Prozent", 0.25, "percent"}, ¬
		{"Wissenschaftlich", 12345.678, "scientific"}, ¬
		{"Bruch", 0.75, "fraction"}, ¬
		{"Text", "0123", "text"}, ¬
		{"Zahlensystem", 255, "numeral system"}, ¬
		{"Ankreuzfeld", true, "checkbox"}, ¬
		{"Bewertung", 3, "rating"}, ¬
		{"Schieberegler", 60, "slider"}, ¬
		{"Schrittwert", 7, "stepper"}, ¬
		{"Einblendmenü", "eins", "pop up menu"}, ¬
		{"Datum formatiert", "01.03.2024", "date and time"}, ¬
		{"Dauer formatiert", "1h 30m", "duration"}}

	tell application id "com.apple.Numbers"
		set doc to make new document
		try
			tell sheet 1 of doc
				set name to "Formate"
				tell table 1
					set name to "Formate"
					set row count to (count of plan) + 2
					set column count to 3
					set value of cell "A1" to "Format"
					set value of cell "B1" to "Wert"
					set value of cell "C1" to "Notiz"

					repeat with i from 1 to count of plan
						set entry to item i of plan
						set r to i + 1
						set value of cell ("A" & r) to item 1 of entry
						set value of cell ("B" & r) to item 2 of entry
						set formatName to item 3 of entry
						try
							my applyFormat(cell ("B" & r) of it, formatName)
							set end of applied to formatName
						on error
							set value of cell ("C" & r) to "refused"
						end try
					end repeat

					-- A date, and a duration, both of which are their own
					-- storage type rather than a format over a number.
					set stamp to current date
					set year of stamp to 2024
					set month of stamp to March
					set day of stamp to 1
					set time of stamp to 9 * hours + 30 * minutes
					set value of cell ("A" & ((count of plan) + 2)) to "Datum"
					set value of cell ("B" & ((count of plan) + 2)) to stamp
				end tell

				-- Merges of four shapes, in a table of their own so the merge
				-- store belongs to nothing else. Three cells wide and three
				-- rows tall rather than two of each: a merge of exactly two
				-- cells cannot distinguish "the range" from "the range minus
				-- its first cell", and the whole question is which of those is
				-- written down. The last one has no value in it, because a
				-- merge is a property of the table and not of a cell.
				--
				-- Each value is written *before* its merge. Written after, it
				-- comes back out: `merge range "B2:D2"` followed by
				-- `set value of cell "B2"` leaves Numbers reporting a merge of
				-- C2:D2 with B2 beside it, which is what the document then
				-- says too. Observed, and worth not tripping over twice.
				set joined to make new table with properties {row count:9, column count:7}
				tell joined
					set name to "Verbunden"
					set value of cell "A1" to "Verbund"
					set value of cell "B2" to "quer"
					merge range "B2:D2"
					set value of cell "B4" to "hoch"
					merge range "B4:B6"
					set value of cell "D4" to "rechteckig"
					merge range "D4:F5"
					merge range "B8:C8"
				end tell

				-- A whole column given one format, which is where a per-column
				-- default is supposed to live.
				set columnwise to make new table with properties {row count:4, column count:2}
				tell columnwise
					set name to "Spaltenformat"
					set value of cell "A1" to "Betrag"
					set value of cell "A2" to 10
					set value of cell "A3" to 20.5
					set value of cell "A4" to 30
					try
						set format of column 1 to currency
					end try
					-- An explicit row height and column width, so that a
					-- reader can tell "sized by hand" from "the table's
					-- default", which is what every row of every other table
					-- here is.
					set width of column 2 to 150
					set height of row 3 to 40
				end tell
			end tell

			set AppleScript's text item delimiters to " "
			set answer to "formats: " & (applied as text)
			set AppleScript's text item delimiters to ""
			with timeout of 600 seconds
				save doc in POSIX file target
			end timeout
		on error message number code
			try
				close doc saving no
			end try
			error message number code
		end try
		close doc saving no
	end tell
	return answer
end run

-- `set format of <cell> to <enumerator>` needs the enumerator as a literal, so
-- the mapping from a name to one is written out rather than computed.
on applyFormat(theCell, formatName)
	tell application id "com.apple.Numbers"
		if formatName is "automatic" then
			set format of theCell to automatic
		else if formatName is "number" then
			set format of theCell to number
		else if formatName is "currency" then
			set format of theCell to currency
		else if formatName is "percent" then
			set format of theCell to percent
		else if formatName is "scientific" then
			set format of theCell to scientific
		else if formatName is "fraction" then
			set format of theCell to fraction
		else if formatName is "text" then
			set format of theCell to text
		else if formatName is "numeral system" then
			set format of theCell to numeral system
		else if formatName is "checkbox" then
			set format of theCell to checkbox
		else if formatName is "rating" then
			set format of theCell to rating
		else if formatName is "slider" then
			set format of theCell to slider
		else if formatName is "stepper" then
			set format of theCell to stepper
		else if formatName is "pop up menu" then
			set format of theCell to pop up menu
		else if formatName is "date and time" then
			set format of theCell to date and time
		else if formatName is "duration" then
			set format of theCell to duration
		else
			error "unknown format " & formatName
		end if
	end tell
end applyFormat
