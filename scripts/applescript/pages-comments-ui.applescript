-- Two comments, via Insert > Comment. NEEDS AN UNLOCKED SCREEN (see
-- pages-footnotes-ui.applescript for the ground rules).
--
-- The one rule everything else here hangs on: **Escape CANCELS the comment
-- popover**, text and all. Committing it means clicking somewhere else — the
-- window's splitter group is always there and always harmless. The first run
-- against a live Pages lost a comment to Escape and doubled its text on the
-- retry; the fixture keeps that scar, and the tests only assert anchors.

on run argv
	set target to item 1 of argv
	tell application id "com.apple.Pages"
		activate
		set d to make new document
		set body text of d to "Kommentierte Woerter verdienen Aufmerksamkeit. Manche mehr als andere."
		with timeout of 600 seconds
			save d in POSIX file target
		end timeout
	end tell
	delay 1
	tell application "System Events"
		tell process "Pages"
			set frontmost to true
			delay 0.5
			-- first word
			click menu item "Select All" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 0.3
			key code 123
			delay 0.3
			key code 124 using {option down, shift down} -- select one word
			delay 0.3
			click menu item "Comment" of menu 1 of menu bar item "Insert" of menu bar 1
			delay 1.2
			keystroke "Erster Kommentar am ersten Wort."
			delay 0.5
			click splitter group 1 of window 1 -- commit; Escape would cancel
			delay 0.8
			-- last word
			click menu item "Select All" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 0.3
			key code 124
			delay 0.3
			key code 123 using {option down, shift down}
			delay 0.3
			click menu item "Comment" of menu 1 of menu bar item "Insert" of menu bar 1
			delay 1.2
			keystroke "Zweiter Kommentar am letzten Wort."
			delay 0.5
			click splitter group 1 of window 1
			delay 0.8
		end tell
	end tell
	tell application id "com.apple.Pages"
		with timeout of 600 seconds
			save document 1
		end timeout
		close document 1 saving yes
	end tell
	return "two comments, first and last word"
end run
