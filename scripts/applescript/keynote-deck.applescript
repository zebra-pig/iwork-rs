-- A deck whose slides differ in every way Keynote lets a script vary them.
--
-- Keynote 15.3.1 calls these things *slide layouts*, not master slides: a slide
-- has a `base layout` property and the document has `slide layout` elements.
-- The older `master slide` vocabulary is gone from the dictionary, so anything
-- addressing it fails outright rather than quietly.
--
-- Layout names come from the theme and are localised, so each is looked up by
-- name with an index to fall back on.
--
-- `skipped` is set after the slide exists. Passing it to `make new slide with
-- properties` is accepted and then ignored — the slide comes back with
-- skipped = false — which is the kind of thing worth writing down once.

on layoutFor(wanted, fallbackIndex, doc)
	tell application id "com.apple.Keynote"
		try
			return slide layout wanted of doc
		on error
			return slide layout fallbackIndex of doc
		end try
	end tell
end layoutFor

on run argv
	set target to item 1 of argv
	set imageFile to item 2 of argv

	tell application id "com.apple.Keynote"
		set doc to make new document
		try
			tell slide 1 of doc
				set object text of default title item to "Wovon dieses Deck handelt"
				set object text of default body item to "Erster Punkt" & return & ¬
					"Zweiter Punkt mit Umlaut: Größe" & return & "Dritter Punkt 🎬"
				set presenter notes to "Notizen zur ersten Folie. Nicht auf der Folie sichtbar."
			end tell

			set bullets to my layoutFor("Title & Bullets", 4, doc)
			set bulletSlide to make new slide at end of slides of doc with properties {base layout:bullets}
			tell bulletSlide
				set object text of default title item to "Zahlen"
				set object text of default body item to "Eins" & return & "Zwei" & return & "Drei"
				set presenter notes to "Hier langsam sprechen."
			end tell

			set statement to my layoutFor("Statement", 12, doc)
			set statementSlide to make new slide at end of slides of doc with properties {base layout:statement}
			tell statementSlide
				try
					set object text of default title item to "Eine Behauptung"
				end try
				set presenter notes to "Eine Folie ohne Aufzählung."
			end tell

			set titleOnly to my layoutFor("Title Only", 10, doc)
			set skipSlide to make new slide at end of slides of doc with properties {base layout:titleOnly}
			tell skipSlide
				try
					set object text of default title item to "Übersprungen"
				end try
				set presenter notes to "Diese Folie ist übersprungen und darf im Vortrag fehlen."
				set skipped to true
			end tell

			make image slides doc files {POSIX file imageFile}

			set answer to ((count of slides of doc) as text) & " slides, skipped=" & ¬
				(skipped of slide 4 of doc as text) & ", layouts=" & ¬
				(count of slide layouts of doc)
			with timeout of 600 seconds
				save doc in POSIX file target
			end timeout
		on error message number code
			try
				close doc saving no
			end try
			error message number code
		end try
		close doc saving no
	end tell
	return answer
end run
