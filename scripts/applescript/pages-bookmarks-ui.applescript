-- Two bookmarks, via Insert > Bookmark. NEEDS AN UNLOCKED SCREEN (see
-- pages-footnotes-ui.applescript).
--
-- A bookmark archive stores a UUID and two small integers — no name; whatever
-- the sidebar shows is derived from the text. The table also keeps terminator
-- entries (an index with no reference, where a bookmark's run ends), which a
-- reader must not count as bookmarks.

on run argv
	set target to item 1 of argv
	tell application id "com.apple.Pages"
		activate
		set d to make new document
		set body text of d to "Ein Lesezeichen sitzt vorn. Ein zweites Lesezeichen sitzt hinten."
		with timeout of 600 seconds
			save d in POSIX file target
		end timeout
	end tell
	delay 1
	tell application "System Events"
		tell process "Pages"
			set frontmost to true
			click menu item "Select All" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 0.3
			key code 123
			delay 0.3
			key code 124 using {option down, shift down}
			delay 0.3
			click menu item "Bookmark" of menu 1 of menu bar item "Insert" of menu bar 1
			delay 1.2
			key code 53
			delay 0.5
			click menu item "Select All" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 0.3
			key code 124
			delay 0.3
			key code 123 using {option down, shift down}
			delay 0.3
			click menu item "Bookmark" of menu 1 of menu bar item "Insert" of menu bar 1
			delay 1.2
			key code 53
			delay 0.5
		end tell
	end tell
	tell application id "com.apple.Pages"
		with timeout of 600 seconds
			save document 1
		end timeout
		close document 1 saving yes
	end tell
	return "two bookmarks, first and last word"
end run
