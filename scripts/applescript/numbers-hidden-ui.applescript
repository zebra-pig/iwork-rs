-- Manually hidden rows and a hidden column. Selection is scriptable in
-- Numbers (`set selection range`), so only the Table menu needs the UI —
-- which still NEEDS AN UNLOCKED SCREEN (see pages-footnotes-ui.applescript).
--
-- The menu item is named for the selection ("Hide 3 Rows"), so the row count
-- here and the menu name have to agree.

on run argv
	set target to item 1 of argv
	tell application id "com.apple.Numbers"
		activate
		set d to make new document
		tell table 1 of sheet 1 of d
			repeat with i from 1 to 8
				set value of cell ("A" & i) to "Zeile " & i
				set value of cell ("B" & i) to i * 10
			end repeat
			set selection range to range "A3:A5"
		end tell
		with timeout of 600 seconds
			save d in POSIX file target
		end timeout
	end tell
	delay 1
	tell application "System Events"
		tell process "Numbers"
			set frontmost to true
			click menu item "Hide 3 Rows" of menu 1 of menu bar item "Table" of menu bar 1
			delay 1
		end tell
	end tell
	tell application id "com.apple.Numbers"
		tell table 1 of sheet 1 of document 1 to set selection range to range "B7:B7"
	end tell
	delay 0.5
	tell application "System Events"
		tell process "Numbers"
			click menu item "Hide Column" of menu 1 of menu bar item "Table" of menu bar 1
			delay 1
		end tell
	end tell
	tell application id "com.apple.Numbers"
		with timeout of 600 seconds
			save document 1
		end timeout
		close document 1 saving yes
	end tell
	return "three rows and a column hidden by hand"
end run
