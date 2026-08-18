-- Keynote's PowerPoint door, both ways.
--
--   keynote-powerpoint.applescript export <document.key>  <out.pptx>
--   keynote-powerpoint.applescript import <document.pptx> <out.key>
--
-- The only route to a transition parameter the scripting dictionary will not
-- set. `transition settings` has four members — effect, duration, delay,
-- automatic — and no direction, so a deck with a direction in it cannot be made
-- by a script. It can be made by the *importer*: PowerPoint's `<p:push dir="u"/>`
-- carries one, and what Keynote writes when it reads that is the app's own
-- answer to "which value means up". See `scripts/transition-direction-probe.sh`.
--
-- The document is opened through launch services and then waited for by name,
-- for the reason `slide-oracle.applescript` records: `open POSIX file …` inside
-- a tell block can take longer than the Apple event will wait, and a restored
-- session means `document 1` is somebody else's.

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
	set mode to item 1 of argv
	set src to item 2 of argv
	set dst to item 3 of argv
	set wanted to my basename(src)
	set alsoWanted to my stem(wanted)

	do shell script "open -g -b com.apple.Keynote " & quoted form of src

	tell application id "com.apple.Keynote"
		set doc to missing value
		repeat 120 times
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
		if doc is missing value then error "Keynote did not open " & src number 8010

		set report to ""
		try
			with timeout of 900 seconds
				if mode is "export" then
					export doc to POSIX file dst as Microsoft PowerPoint
					set report to "exported"
				else
					-- Report what the app makes of the imported transitions
					-- before saving, so a run says something even if the deck
					-- is never read back.
					repeat with s in slides of doc
						set t to transition properties of s
						set report to report & ((slide number of s) as text) & "=" & ¬
							((transition effect of t) as text) & " "
					end repeat
					save doc in POSIX file dst
				end if
			end timeout
		on error message number code
			try
				close doc saving no
			end try
			error message number code
		end try
		close doc saving no
	end tell
	return report
end run
