-- Several paragraphs, each formatted differently from the last.
--
-- Pages exposes no bold and no italic. Its scripting dictionary gives rich text
-- exactly three properties — `font`, `size` and `color` (sdef, class "rich
-- text") — so weight and slant are reached the way the dictionary allows, by
-- naming a face: Helvetica-Bold, Helvetica-Oblique. That is a different edit
-- from the bold *toggle* the style graph carries, and worth knowing when this
-- fixture is used to check what a character style holds.
--
-- Colour is a list of three 0–65535 channels, not 0–1.

on run argv
	set target to item 1 of argv
	tell application id "com.apple.Pages"
		set d to make new document
		try
			set body text of d to "Überschrift" & return & ¬
				"Ein roter Absatz, damit die Farbe irgendwo im Dokument steht." & return & ¬
				"Ein kursiver Absatz, gesetzt über den Schriftschnitt." & return & ¬
				"Ein ganz gewöhnlicher Absatz zum Vergleich."
			tell body text of d
				set font of paragraph 1 to "Helvetica-Bold"
				set size of paragraph 1 to 24
				set color of paragraph 2 to {58000, 8000, 8000}
				set font of paragraph 3 to "Helvetica-Oblique"
				set size of paragraph 4 to 11
			end tell
			with timeout of 600 seconds
				save d in POSIX file target
			end timeout
		on error message number code
			try
				close d saving no
			end try
			error message number code
		end try
		close d saving no
	end tell
	return "4 paragraphs, 3 of them styled"
end run
