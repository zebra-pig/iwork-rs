-- Ask Pages how it *draws* each paragraph of a document's body: its size, its
-- font and its colour, one paragraph to a line, tab separated.
--
-- This is the oracle for text styling, and the reason it exists is that
-- nothing else can answer the question. A style this crate writes can be
-- structurally perfect, pass `iwork check`, survive the app's own save — and
-- still not be what the app draws the paragraph with. The archive says what
-- was asked for; only the app says what was done.
--
-- Opened the way `check-pages.applescript` opens a document and for the same
-- reason: told to `open` one, Pages often never replies to the event.

on run argv
	set target to item 1 of argv
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

	set out to ""
	set paraSizes to {}
	set paraFonts to {}
	set paraColors to {}
	with timeout of 600 seconds
		tell application id "com.apple.Pages"
			-- Asked of the whole range at once: a single paragraph held in a
			-- variable answers neither `size` nor `font`, and the range does.
			try
				set paraSizes to size of every paragraph of body text of doc
			end try
			try
				set paraFonts to font of every paragraph of body text of doc
			end try
			try
				set paraColors to color of every paragraph of body text of doc
			end try
		end tell
		repeat with i from 1 to (count of paraSizes)
			set fn to "?"
			if (count of paraFonts) is greater than or equal to i then set fn to (item i of paraFonts) as text
			-- Colour as three 16-bit channels, the way the app reports it.
			set co to "?"
			if (count of paraColors) is greater than or equal to i then
				set c to item i of paraColors
				try
					set co to ((item 1 of c) as text) & "," & ((item 2 of c) as text) & "," & ((item 3 of c) as text)
				end try
			end if
			set out to out & ((item i of paraSizes) as text) & tab & fn & tab & co & linefeed
		end repeat
	end timeout
	return out
end run

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
