-- Ask Pages or Keynote how it *draws* each word: the word, its size, its font
-- and its colour, one word to a line, tab separated. For Pages the words of the
-- body; for Keynote the words of every text item of every slide, in order.
--
-- The oracle for a run of characters with a look of its own inside a
-- paragraph (`Document::format_text`), as `paragraph-oracle` is for whole
-- paragraphs. Font tells bold and italic: the app answers the face it draws,
-- "Helvetica-Bold" for Helvetica with the bold toggle set.

on run argv
	set target to item 1 of argv
	set bundle to "com.apple.Pages"
	if target ends with ".key" then set bundle to "com.apple.Keynote"
	do shell script "open -g -b " & bundle & " " & quoted form of target

	set wanted to my basename(target)
	set alsoWanted to my stem(wanted)
	set doc to missing value
	repeat 60 times
		tell application id bundle
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
	if doc is missing value then error "the app did not open " & target number 8010

	set out to ""
	with timeout of 600 seconds
		if bundle is "com.apple.Pages" then
			tell application id "com.apple.Pages"
				set ws to every word of body text of doc
				set ss to size of every word of body text of doc
				set fs to font of every word of body text of doc
				set cs to color of every word of body text of doc
			end tell
			set out to out & my rows(ws, ss, fs, cs)
		else
			tell application id "com.apple.Keynote"
				set boxes to {}
				repeat with s in slides of doc
					repeat with t in text items of s
						try
							set ws to every word of object text of t
							set ss to size of every word of object text of t
							set fs to font of every word of object text of t
							set cs to color of every word of object text of t
							set end of boxes to {ws, ss, fs, cs}
						end try
					end repeat
				end repeat
			end tell
			repeat with b in boxes
				set out to out & my rows(item 1 of b, item 2 of b, item 3 of b, item 4 of b)
			end repeat
		end if
	end timeout
	return out
end run

on rows(ws, ss, fs, cs)
	set out to ""
	repeat with i from 1 to (count of ws)
		set c to item i of cs
		set co to ((item 1 of c) as text) & "," & ((item 2 of c) as text) & "," & ((item 3 of c) as text)
		set out to out & ((item i of ws) as text) & tab & ((item i of ss) as text) & tab & ((item i of fs) as text) & tab & co & linefeed
	end repeat
	return out
end rows

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
