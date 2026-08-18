-- Everything an app will say about the objects placed in a document, as TSV.
--
-- This is the oracle Phase 3 is measured against, the way
-- `table-oracle.applescript` is Phase 1's: whatever the app reports for an
-- object's rectangle is what that rectangle *is*, and a decoder that disagrees
-- is wrong. It is what proves the crate's composition rule for masked images —
-- the app reports the mask's window, not the picture's own rectangle — and what
-- a geometry write is checked against afterwards.
--
-- Output, one record per line, tab-separated:
--
--   container <index>  <name>
--   item      <container index>  <class>  <x>  <y>  <width>  <height>  <locked>  <rotation>  <opacity>
--
-- `rotation` and `opacity` come back as `-` on the classes that do not have
-- them: `iWork item` carries only position, size and lock, and a table has
-- neither.
--
-- The body is written out once per app rather than factored into a handler,
-- because a property is fetched by sending an Apple event and the event has to
-- go to a named application — a handler outside the `tell` block cannot ask.
-- Each property is fetched into a local first: `item 1 of (position of thing)`
-- inside a tell block builds a *reference* the app then refuses to coerce, an
-- error that reads like a missing property and is not one.
--
-- Opened through launch services and then waited for by name, because the app
-- does not always answer the event that asked it to open a document, and
-- because a restored session means `document 1` is somebody else's.

on basename(p)
	set text item delimiters to "/"
	set b to last text item of p
	set text item delimiters to ""
	return b
end basename

on stem(n)
	if n contains "." then
		set text item delimiters to "."
		set parts to text items of n
		set text item delimiters to "."
		set s to (items 1 thru -2 of parts) as text
		set text item delimiters to ""
		return s
	end if
	return n
end stem

on run argv
	set target to item 1 of argv
	set wanted to my basename(target)
	set alsoWanted to my stem(wanted)
	set harvest to {}

	if target ends with ".key" then
		set bundle to "com.apple.Keynote"
	else if target ends with ".pages" then
		set bundle to "com.apple.Pages"
	else if target ends with ".numbers" then
		set bundle to "com.apple.Numbers"
	else
		error "not an iWork document: " & target number 2
	end if

	do shell script "open -g -b " & bundle & " " & quoted form of target

	if bundle is "com.apple.Keynote" then
		tell application id "com.apple.Keynote"
			set doc to missing value
			repeat 60 times
				try
					repeat with d in documents
						if (name of d) is wanted or (name of d) is alsoWanted then
							set doc to d
							exit repeat
						end if
					end repeat
				end try
				if doc is not missing value then exit repeat
				delay 1
			end repeat
			if doc is missing value then error "Keynote did not open " & target number 8010
			with timeout of 900 seconds
				set n to 0
				repeat with s in slides of doc
					set n to n + 1
					set end of harvest to "container" & tab & n & tab & ("slide " & n)
					repeat with thing in (iWork items of s)
						set pos to position of thing
						set px to (item 1 of pos) as text
						set py to (item 2 of pos) as text
						set w to (width of thing) as text
						set h to (height of thing) as text
						set k to (class of thing) as text
						set lk to (locked of thing) as text
						set spin to "-"
						set fade to "-"
						try
							set spin to (rotation of thing) as text
						end try
						try
							set fade to (opacity of thing) as text
						end try
						set end of harvest to "item" & tab & n & tab & k & tab & px & tab & py & tab & w & tab & h & tab & lk & tab & spin & tab & fade
					end repeat
				end repeat
			end timeout
			close doc saving no
		end tell
	else if bundle is "com.apple.Pages" then
		tell application id "com.apple.Pages"
			set doc to missing value
			repeat 60 times
				try
					repeat with d in documents
						if (name of d) is wanted or (name of d) is alsoWanted then
							set doc to d
							exit repeat
						end if
					end repeat
				end try
				if doc is not missing value then exit repeat
				delay 1
			end repeat
			if doc is missing value then error "Pages did not open " & target number 8010
			with timeout of 900 seconds
				set n to 0
				repeat with pg in pages of doc
					set n to n + 1
					set end of harvest to "container" & tab & n & tab & ("page " & n)
					repeat with thing in (iWork items of pg)
						set pos to position of thing
						set px to (item 1 of pos) as text
						set py to (item 2 of pos) as text
						set w to (width of thing) as text
						set h to (height of thing) as text
						set k to (class of thing) as text
						set lk to (locked of thing) as text
						set spin to "-"
						set fade to "-"
						try
							set spin to (rotation of thing) as text
						end try
						try
							set fade to (opacity of thing) as text
						end try
						set end of harvest to "item" & tab & n & tab & k & tab & px & tab & py & tab & w & tab & h & tab & lk & tab & spin & tab & fade
					end repeat
				end repeat
			end timeout
			close doc saving no
		end tell
	else
		tell application id "com.apple.Numbers"
			set doc to missing value
			repeat 60 times
				try
					repeat with d in documents
						if (name of d) is wanted or (name of d) is alsoWanted then
							set doc to d
							exit repeat
						end if
					end repeat
				end try
				if doc is not missing value then exit repeat
				delay 1
			end repeat
			if doc is missing value then error "Numbers did not open " & target number 8010
			with timeout of 900 seconds
				set n to 0
				repeat with sh in sheets of doc
					set n to n + 1
					set end of harvest to "container" & tab & n & tab & (name of sh)
					repeat with thing in (iWork items of sh)
						set pos to position of thing
						set px to (item 1 of pos) as text
						set py to (item 2 of pos) as text
						set w to (width of thing) as text
						set h to (height of thing) as text
						set k to (class of thing) as text
						set lk to (locked of thing) as text
						set spin to "-"
						set fade to "-"
						try
							set spin to (rotation of thing) as text
						end try
						try
							set fade to (opacity of thing) as text
						end try
						set end of harvest to "item" & tab & n & tab & k & tab & px & tab & py & tab & w & tab & h & tab & lk & tab & spin & tab & fade
					end repeat
				end repeat
			end timeout
			close doc saving no
		end tell
	end if

	set text item delimiters to linefeed
	set answer to harvest as text
	set text item delimiters to ""
	return answer
end run
