-- Builds — the Animate inspector, since no dictionary has a build vocabulary.
-- NEEDS AN UNLOCKED SCREEN (see pages-footnotes-ui.applescript).
--
-- What the accessibility tree taught, at a run apiece:
--   * Keynote's toolbar buttons have NO AXName — match on `description`.
--   * The inspector opens from View > Inspector > Animate; its "Add an
--     Effect" IS named, but lives deep — walk `entire contents` and match,
--     because a whose-clause cannot be applied to `entire contents`.
--   * Build In / Action / Build Out are AXRadioButtons, again by description.
--   * The effect chooser is a flat list of buttons named as the effects are.
--   * Selecting objects: click the canvas (splitter group 1), then
--     Select All — Tab selection needs a focus no script reliably has.

on run argv
	set target to item 1 of argv
	tell application id "com.apple.Keynote"
		activate
		set d to make new document
		tell slide 1 of d
			set object text of default title item to "Ein Deck mit Builds"
			make new shape at end with properties {position:{700, 500}, width:400, height:300}
		end tell
		with timeout of 600 seconds
			save d in POSIX file target
		end timeout
	end tell
	delay 1.5
	tell application "System Events"
		tell process "Keynote"
			set frontmost to true
			click splitter group 1 of window 1
			delay 0.5
			click menu item "Select All" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 1
			click menu item "Animate" of menu 1 of menu item "Inspector" of menu 1 of menu bar item "View" of menu bar 1
			delay 1.5
			my clickNamed("Add an Effect")
			delay 1.5
			my clickNamed("Dissolve")
			delay 2
			-- and a build-out for the same objects
			click splitter group 1 of window 1
			delay 0.3
			click menu item "Select All" of menu 1 of menu bar item "Edit" of menu bar 1
			delay 1
			my clickDescribed("Build Out")
			delay 1.5
			my clickNamed("Add an Effect")
			delay 1.5
			my clickNamed("Disappear")
			delay 2
		end tell
	end tell
	tell application id "com.apple.Keynote"
		with timeout of 600 seconds
			save document 1
		end timeout
		close document 1 saving yes
	end tell
	return "dissolve in, disappear out, every object"
end run

on clickNamed(wanted)
	tell application "System Events"
		tell process "Keynote"
			repeat with e in (entire contents of window 1)
				try
					if name of e is wanted then
						click e
						return
					end if
				end try
			end repeat
		end tell
	end tell
	error "no element named " & wanted
end clickNamed

on clickDescribed(wanted)
	tell application "System Events"
		tell process "Keynote"
			repeat with e in (entire contents of window 1)
				try
					if role of e is "AXRadioButton" and description of e is wanted then
						click e
						return
					end if
				end try
			end repeat
		end tell
	end tell
	error "no radio button described " & wanted
end clickDescribed
