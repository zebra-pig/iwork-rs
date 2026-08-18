-- Two footnotes, via Insert > Footnote — nothing in the dictionary makes one.
--
-- NEEDS AN UNLOCKED SCREEN. A locked screen exposes zero AX windows and every
-- menu item validates as disabled (PLAN.md ground rule 7); make-fixtures.sh
-- probes before running this. Menu names are as the English UI shows them —
-- on another locale, read `name of every menu item` first and match.
--
-- The moves that matter, learned the expensive way:
--   * Select All then an arrow key is how a cursor lands somewhere useful in a
--     document a script cannot click into.
--   * After typing a footnote the caret is in the *note*; Escape returns it to
--     the body, and Insert > Footnote is disabled until it does.

on run argv
	set target to item 1 of argv
	tell application id "com.apple.Pages"
		activate
		set d to make new document
		set body text of d to "Ein Dokument mit Fussnoten. Die erste Note haengt hier. Und die zweite hier."
		with timeout of 600 seconds
			save d in POSIX file target
		end timeout
	end tell
	delay 1
	tell application "System Events"
		tell process "Pages"
			set frontmost to true
			delay 0.5
			-- first note, at the end of the body
			click menu item "Select All" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 0.5
			key code 124 -- right: collapse to end
			delay 0.5
			click menu item "Footnote" of menu 1 of menu bar item "Insert" of menu bar 1
			delay 1
			keystroke "Erste Fussnote, mit Umlaut: Grösse."
			delay 0.5
			-- second note, mid-sentence
			key code 53 -- Escape: leave the note area
			delay 0.5
			click menu item "Select All" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 0.5
			key code 123 -- left: collapse to start
			delay 0.3
			repeat 26 times
				key code 124
			end repeat
			delay 0.3
			click menu item "Footnote" of menu 1 of menu bar item "Insert" of menu bar 1
			delay 1
			keystroke "Zweite Fussnote, mitten im Satz."
			delay 0.5
		end tell
	end tell
	tell application id "com.apple.Pages"
		with timeout of 600 seconds
			save document 1
		end timeout
		close document 1 saving yes
	end tell
	return "two footnotes, one mid-sentence"
end run
