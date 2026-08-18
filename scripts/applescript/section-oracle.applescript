-- Ask Pages what it thinks a document's sections are.
--
-- Three properties, and they are the only three the dictionary has about
-- document structure: `document body`, a read-only boolean that is false for a
-- page-layout document; the `sections` element; and each section's `body text`.
-- There is no header, no footer, no footnote, no column and no table of
-- contents in the dictionary at all, and no `make new section`.
--
-- `body text of section i` is what pins the section ranges this crate decodes.
-- A section's entry in the body storage's table 17 sits on the character after
-- the `U+0004` that begins it, so section i covers [start(i), start(i+1) - 1)
-- and the break belongs to neither — arithmetic the app either agrees with,
-- character for character, or does not.
--
--   osascript section-oracle.applescript DOCUMENT
--
-- Output, one record per line, tab-separated:
--
--   body    true|false
--   count   N
--   section INDEX  LENGTH  TEXT-WITH-NEWLINES-ESCAPED
--
-- The document is handed to `open(1)` and then waited for by name, for the
-- reason every other script here does it: told to open a document Pages often
-- never answers the event, and `document 1` is the wrong document whenever the
-- app has restored a session or another test binary is driving it.

on run argv
	set target to item 1 of argv
	set harvest to {}

	do shell script "open -g -b com.apple.Pages " & quoted form of target

	set wanted to my basename(target)
	set alsoWanted to my stem(wanted)
	set doc to missing value
	repeat 60 times
		tell application id "com.apple.Pages"
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
	if doc is missing value then error "Pages did not open " & target number 8010

	with timeout of 600 seconds
		tell application id "com.apple.Pages"
			try
				set end of harvest to "body" & tab & ((document body of doc) as text)
			end try
			set total to 0
			try
				set total to count of sections of doc
			end try
			set end of harvest to "count" & tab & (total as text)
			repeat with i from 1 to total
				try
					set body to (body text of section i of doc) as text
					set end of harvest to "section" & tab & (i as text) & tab & ¬
						((length of body) as text) & tab & my escaped(body)
				end try
			end repeat
			close doc saving no
		end tell
	end timeout

	set AppleScript's text item delimiters to linefeed
	set answer to harvest as text
	set AppleScript's text item delimiters to ""
	return answer
end run

-- Newlines and tabs would break the line-per-record format, and a section's
-- text is full of both.
on escaped(t)
	set out to ""
	repeat with c in characters of t
		set n to id of c
		if n is 10 then
			set out to out & "\\n"
		else if n is 13 then
			set out to out & "\\r"
		else if n is 9 then
			set out to out & "\\t"
		else if n is 92 then
			set out to out & "\\\\"
		else
			set out to out & c
		end if
	end repeat
	return out
end escaped

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
