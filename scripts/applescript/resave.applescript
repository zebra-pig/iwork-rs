-- Open a document, have the app save it, and close it.
--
-- The strongest acceptance test available for a part of a document no
-- dictionary can read back. `app-check.sh` proves the app opened the file;
-- this proves the app *read* the edit, kept it through its own model, and
-- wrote it out again — which is a much harder thing to pass by accident. What
-- comes back is a document written entirely by Pages, Numbers or Keynote, and
-- whatever this crate put in is either still in it or is not.
--
--   osascript resave.applescript DOCUMENT
--
-- `save doc` and then `close doc saving yes`. Never `saving no`: on a document
-- that has just been saved to a new location that does not answer for minutes
-- and then deletes the file, which cost three fixtures before it was
-- understood.

on run argv
	set target to item 1 of argv

	do shell script "open -g -b " & my bundleFor(target) & " " & quoted form of target

	set wanted to my basename(target)
	set alsoWanted to my stem(wanted)
	set doc to missing value
	repeat 60 times
		tell application id (my bundleFor(target))
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

	with timeout of 600 seconds
		tell application id (my bundleFor(target))
			save doc
			close doc saving yes
		end tell
	end timeout
	return "resaved " & wanted
end run

on bundleFor(path)
	set base to my basename(path)
	set AppleScript's text item delimiters to "."
	set extension to last text item of base
	set AppleScript's text item delimiters to ""
	if extension is "pages" then return "com.apple.Pages"
	if extension is "numbers" then return "com.apple.Numbers"
	if extension is "key" then return "com.apple.Keynote"
	error "not an iWork document: " & path number 8011
end bundleFor

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
