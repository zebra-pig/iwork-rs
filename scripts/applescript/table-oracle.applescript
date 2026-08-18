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

	set wanted to my basename(target)
	set alsoWanted to my stem(wanted)

	do shell script "open -g -b com.apple.Numbers " & quoted form of target

	-- Wait for *this* document, not for any document. Numbers restores the
	-- session it was quit with, and a restored spreadsheet can still be
	-- arriving when the one that was asked for opens — after which
	-- `document 1` is somebody else's, and the answer is a full, plausible,
	-- entirely wrong reading of another file. Seen: an edited fixture
	-- reporting the values it had before the edit.
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

-- The name Numbers gives a document is one of two things, and which one is not
-- ours to decide: a document the app itself wrote carries the Finder's
-- hide-extension flag and answers "numbers-values", while a file this
-- repository wrote answers "iwork-set-cell.numbers". Both are matched, because
-- the alternative is a harness that works on fixtures and not on output.
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
