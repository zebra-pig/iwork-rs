-- A password-protected document — the only thing in the review-and-metadata
-- area any of the three scripting dictionaries will do.
--
--   osascript pages-locked.applescript TARGET SOURCE
--
-- Copies SOURCE, opens the copy, sets a password with a hint, and lets the app
-- save it. What comes back is an entirely different kind of package: a `.iwpv2`
-- entry at the root, a `.iwph` holding the hint in plain text, every
-- `Index/*.iwa`, every `Data/*` and `Metadata/BuildVersionHistory.plist` turned
-- into ciphertext, and no previews at all. `Metadata/Properties.plist` and
-- `Metadata/DocumentIdentifier` stay readable.
--
-- Three things cost an afternoon between them, and all three are the
-- AppleScript compiler rather than the app:
--
--   `set before to password protected of doc` does not compile, because
--   `before` is a reserved word. The error points at the `to`.
--
--   `set locked to …` compiles and then fails at run time with -10006, because
--   `locked` is the iWork item property of that name. Names that read like
--   plain English are exactly the ones a scripting dictionary has taken.
--
--   `set password pw` does not compile either, with a *variable* in the direct
--   parameter: the compiler reads `password` as the first word of the
--   `password protected` property and stops on the identifier after it. The
--   dictionary's own example uses a literal. So the call is built as text and
--   compiled at run time with `run script`, which is also why the bundle
--   identifier is spliced in rather than held in a variable — terminology for
--   `tell application id someVariable` is not available at compile time either.

on run argv
	set target to item 1 of argv
	set source to item 2 of argv

	do shell script "/bin/cp " & quoted form of source & " " & quoted form of target
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
	if doc is missing value then error "the app did not open " & target number 8010

	tell application id "com.apple.Pages"
		set docName to name of doc
	end tell
	set opening to "tell application id \"com.apple.Pages\" to tell document \"" & docName & "\" to "

	with timeout of 600 seconds
		run script (opening & "set password \"p4ssw0rd\" hint \"the probe\" saving in keychain false")
		set didLock to run script (opening & "get password protected")
		tell application id "com.apple.Pages"
			save doc
			close doc saving yes
		end tell
	end timeout
	if didLock is false then error "Pages did not lock " & target number 8012
	return "locked with p4ssw0rd, hint \"the probe\""
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
