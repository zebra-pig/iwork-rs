-- Everything Keynote will say about a deck's slides, as TSV.
--
-- This is the oracle Phase 8a is measured against. What the app reports for a
-- slide's order, its layout, its title, its body, its presenter notes and its
-- skipped flag is what those things *are*; a reader that disagrees is wrong.
--
-- Output, one record per line, tab-separated, with tabs and line breaks inside
-- a text escaped as \t, \n and \r so that one slide is always one line. The
-- two line breaks are kept apart on purpose: Keynote separates the paragraphs
-- of a body placeholder with a **carriage return**, and a reader that folded
-- them together could not tell that it had them right.
--
--   show   <slide count>  <layout count>  <width>  <height>  <slide numbers showing>  <theme name>
--   layout <index>  <name>
--   slide  <number>  <layout name>  <skipped>  <title showing>  <body showing>  <title>  <body>  <notes>  <transition effect>  <automatic>  <delay>  <duration>
--
-- `title` and `body` come back as `-` when the layout has no such placeholder:
-- `default title item` answers with an *error* rather than an empty shape, so
-- each read stands on its own inside a `try`. An empty placeholder is a present
-- one and reads as the empty string, which is not the same thing.
--
-- Every property is fetched into a local before it is used, for the reason
-- `drawable-oracle.applescript` records: a property expression inside a tell
-- block builds a reference the app then refuses to coerce, and the error reads
-- like a missing property.
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

-- One slide has to be one line, and slide text has newlines in it: a body with
-- three bullets is three paragraphs. Escape rather than drop, so the reader can
-- compare the whole string.
on escape(t)
	set out to t as text
	set text item delimiters to tab
	set parts to text items of out
	set text item delimiters to "\\t"
	set out to parts as text
	set text item delimiters to linefeed
	set parts to text items of out
	set text item delimiters to "\\n"
	set out to parts as text
	set text item delimiters to return
	set parts to text items of out
	set text item delimiters to "\\r"
	set out to parts as text
	set text item delimiters to ""
	return out
end escape

on run argv
	set target to item 1 of argv
	if target does not end with ".key" then error "not a Keynote document: " & target number 2
	set wanted to my basename(target)
	set alsoWanted to my stem(wanted)
	set harvest to {}

	do shell script "open -g -b com.apple.Keynote " & quoted form of target

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
			try
				set slideCount to count of slides of doc
				set layoutCount to count of slide layouts of doc
				set showWidth to width of doc
				set showHeight to height of doc
				set numbering to (slide numbers showing of doc) as text
				set themeName to "-"
				try
					set themeName to name of document theme of doc
				end try
				set end of harvest to "show" & tab & slideCount & tab & layoutCount & ¬
					tab & showWidth & tab & showHeight & tab & numbering & tab & themeName

				repeat with i from 1 to layoutCount
					set lay to slide layout i of doc
					set end of harvest to "layout" & tab & i & tab & (name of lay)
				end repeat

				repeat with s in slides of doc
					set n to slide number of s
					set layoutName to "-"
					try
						set layoutName to name of base layout of s
					end try
					set isSkipped to (skipped of s) as text
					set titleShowing to (title showing of s) as text
					set bodyShowing to (body showing of s) as text

					set titleText to "-"
					try
						set titleText to my escape(object text of default title item of s)
					end try
					set bodyText to "-"
					try
						set bodyText to my escape(object text of default body item of s)
					end try
					set noteText to my escape(presenter notes of s)

					set fxName to "-"
					set autoAdvance to "-"
					set theDelay to "-"
					set fxDuration to "-"
					try
						set t to transition properties of s
						set fxName to (transition effect of t) as text
						set autoAdvance to (automatic transition of t) as text
						set theDelay to (transition delay of t) as text
						set fxDuration to (transition duration of t) as text
					end try

					set end of harvest to "slide" & tab & n & tab & layoutName & tab & ¬
						isSkipped & tab & titleShowing & tab & bodyShowing & tab & ¬
						titleText & tab & bodyText & tab & noteText & tab & ¬
						fxName & tab & autoAdvance & tab & theDelay & tab & fxDuration
				end repeat
			on error message number code
				try
					close doc saving no
				end try
				error message number code
			end try
			close doc saving no
		end timeout
	end tell

	set text item delimiters to linefeed
	set answer to harvest as text
	set text item delimiters to ""
	return answer
end run
