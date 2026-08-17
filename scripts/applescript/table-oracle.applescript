-- Everything Numbers will say about the tables in a document, as TSV.
--
-- This is the oracle Phase 1 is measured against: whatever the app reports for
-- a cell is what that cell *is*, and a decoder that disagrees is wrong. It is
-- deliberately more than `check-numbers.applescript` reads — that one confirms
-- a document opens, this one enumerates the whole table model so a reader can
-- be compared to it cell by cell.
--
-- Speed comes from asking for a whole table at a time: `value of every cell` is
-- one Apple event where a loop over the cells would be thousands, and the large
-- fixture has 2711 of them. Six events per table, then all the pairing up
-- happens locally.
--
-- Output, one record per line, tab-separated:
--
--   sheet   <name>
--   table   <name>  <rows>  <columns>  <header rows>  <header columns>  <footer rows>
--   row     <index> <height>
--   column  <index> <width>
--   cell    <A1>    <class> <value>  <formatted>  <format>  <formula>
--
-- A cell whose value is `missing value` is reported with class `empty`. Values
-- are written with `as text`, which is locale-dependent for dates and reals —
-- the comparison on the other side knows that and compares those loosely.
--
-- Opened through launch services and then waited for, because the app does not
-- always answer the event that asked it to open a document.

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

	with timeout of 900 seconds
		tell application id "com.apple.Numbers"
			try
				repeat with s in sheets of doc
					set end of harvest to "sheet" & tab & (name of s)
					repeat with t in tables of s
						set end of harvest to "table" & tab & (name of t) & tab & ¬
							(row count of t) & tab & (column count of t) & tab & ¬
							(header row count of t) & tab & (header column count of t) & tab & ¬
							(footer row count of t)

						set heights to height of every row of t
						repeat with i from 1 to count of heights
							set end of harvest to "row" & tab & i & tab & (item i of heights)
						end repeat
						set widths to width of every column of t
						repeat with i from 1 to count of widths
							set end of harvest to "column" & tab & i & tab & (item i of widths)
						end repeat

						set names to name of every cell of t
						set values to value of every cell of t
						set shown to formatted value of every cell of t
						set formats to format of every cell of t
						set formulas to formula of every cell of t

						repeat with i from 1 to count of names
							set v to item i of values
							set theClass to "empty"
							set asText to ""
							if v is not missing value then
								set theClass to (class of v) as text
								try
									set asText to v as text
								on error
									set asText to "?"
								end try
							end if
							set end of harvest to "cell" & tab & (item i of names) & tab & ¬
								theClass & tab & asText & tab & ¬
								my flatten(item i of shown) & tab & ¬
								my flatten(item i of formats) & tab & ¬
								my flatten(item i of formulas)
						end repeat
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

-- `missing value` as the empty string, everything else as one line of text.
on flatten(v)
	if v is missing value then return ""
	try
		set t to v as text
	on error
		return "?"
	end try
	set AppleScript's text item delimiters to {tab, linefeed, return}
	set pieces to text items of t
	set AppleScript's text item delimiters to " "
	set t to pieces as text
	set AppleScript's text item delimiters to ""
	return t
end flatten
