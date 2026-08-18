-- Track Changes on, then an insertion, a deletion and a replacement, none of
-- them accepted. NEEDS AN UNLOCKED SCREEN (see pages-footnotes-ui.applescript).
--
-- `sdef` finds the word "change" nowhere in Pages' dictionary; the Edit menu
-- is the only way in. The edits themselves are plain keystrokes — under
-- tracking, delete does not remove characters, it marks them, which is exactly
-- what the fixture is for.

on run argv
	set target to item 1 of argv
	tell application id "com.apple.Pages"
		activate
		set d to make new document
		set body text of d to "Der urspruengliche Satz bleibt hier stehen. Dieser Satz wird veraendert werden. Ein dritter Satz rundet ab."
		with timeout of 600 seconds
			save d in POSIX file target
		end timeout
	end tell
	delay 1
	tell application "System Events"
		tell process "Pages"
			set frontmost to true
			click menu item "Track Changes" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 1.5
			-- an insertion at the end
			click menu item "Select All" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 0.3
			key code 124
			delay 0.3
			keystroke " Eingefuegter Nachsatz."
			delay 0.5
			-- a deletion: the last word
			key code 123 using {option down, shift down}
			delay 0.3
			key code 51 -- forward through the marked text
			delay 0.5
			-- a replacement: type over the first word
			key code 126 using {command down}
			delay 0.3
			key code 124 using {option down, shift down}
			delay 0.3
			keystroke "Ersetzter"
			delay 0.5
		end tell
	end tell
	tell application id "com.apple.Pages"
		with timeout of 600 seconds
			save document 1
		end timeout
		close document 1 saving yes
	end tell
	return "tracking on: insertion, deletion, replacement"
end run
