-- How Numbers *draws* the cells of a document: background, font and text
-- colour, as TSV.
--
-- `table-oracle.applescript` says what a cell holds; this says what it looks
-- like, which is a different question and the only one styling can be
-- measured against. A fill can be in the archive, pass `iwork check`, survive
-- the app's own save, and still not be what the cell is painted with — the
-- archive says what was asked for, and only the app says what was done.
--
-- Output, one record per line, tab-separated:
--
--   table   <name>  <rows>  <columns>  <header rows>  <header columns>
--   cell    <A1>    <background r,g,b>  <font name>  <font size>  <text r,g,b>
--
-- Colours are three 16-bit channels as the app reports them; a cell with no
-- fill reports `none`. Only the first `limit` rows of each table are read
-- (argument 2, default 8): the point is the header and a few body rows, and a
-- cell at a time is slow.
--
-- Opened through launch services and then waited for, like every other oracle
-- here, because the app does not always answer the event that asked it to
-- open a document.

on run argv
	set target to item 1 of argv
	set rowLimit to 8
	if (count of argv) > 1 then set rowLimit to (item 2 of argv) as integer
	set harvest to {}

	set wanted to my basename(target)
	set alsoWanted to my stem(wanted)

	do shell script "open -g -b com.apple.Numbers " & quoted form of target

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
			repeat with s in sheets of doc
				repeat with t in tables of s
					set rowTotal to row count of t
					set columnTotal to column count of t
					set end of harvest to "table" & tab & (name of t) & tab & rowTotal & tab & columnTotal & tab & ¬
						(header row count of t) & tab & (header column count of t)
					set lastRow to rowTotal
					if lastRow > rowLimit then set lastRow to rowLimit
					repeat with r from 1 to lastRow
						repeat with c from 1 to columnTotal
							set theCell to cell c of row r of t
							set cellName to name of theCell
							set bg to "none"
							set fontName to "?"
							set fontSize to "?"
							set ink to "?"
							try
								set v to background color of theCell
								if v is not missing value then set bg to my triple(v)
							end try
							try
								set fontName to (font name of theCell) as text
							end try
							try
								set fontSize to (font size of theCell) as text
							end try
							try
								set ink to my triple(text color of theCell)
							end try
							set end of harvest to "cell" & tab & cellName & tab & bg & tab & fontName & tab & fontSize & tab & ink
						end repeat
					end repeat
				end repeat
			end repeat
		end tell
	end timeout

	set AppleScript's text item delimiters to linefeed
	set answer to harvest as text
	set AppleScript's text item delimiters to ""
	return answer
end run

on triple(v)
	return ((item 1 of v) as text) & "," & ((item 2 of v) as text) & "," & ((item 3 of v) as text)
end triple

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
