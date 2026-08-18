-- A slide carrying one of every drawable a script can put on it.
--
-- Keynote is the only one of the three apps that will create a drawable from a
-- script: Pages answers `make new shape` with "Don't know how to create
-- TMAScriptShapeInfoProxy" and Numbers does the same. `TSD` is cross-app — a
-- shape is the same archive wherever it sits — so this deck is the corpus's
-- shape, line, text-item and image fixture for all three.
--
-- What the app would *not* do, probed rather than assumed: `make new group` and
-- `make new movie` both answer "ok" and then put nothing on the slide. Groups
-- and movies therefore stay read-only in this crate, exercised by the themes
-- that ship with them.
--
-- Each object is given properties the app can read back — position, size,
-- rotation, opacity, reflection, lock — so that a decoder can be measured
-- against the app object by object. `locked` is set last: a locked object
-- refuses every other change.
--
-- One AppleScript trap, found the hard way: naming the shape variable `plain`
-- makes the *app* answer "Access not allowed" (-10003) rather than the parser
-- complain — `plain` is a term the dialect already owns. Every local here has a
-- name nothing else claims.

on run argv
	set target to item 1 of argv
	set imageFile to item 2 of argv
	set squareFile to item 3 of argv

	tell application id "com.apple.Keynote"
		set doc to make new document
		try
			tell slide 1 of doc
				set boxOne to make new shape at end of shapes with properties ¬
					{position:{100, 100}, width:300, height:200}
				set object text of boxOne to "Ein Rechteck"

				set boxTwo to make new shape at end of shapes with properties ¬
					{position:{500, 100}, width:220, height:180}
				set rotation of boxTwo to 30

				set boxThree to make new shape at end of shapes with properties ¬
					{position:{800, 100}, width:200, height:200}
				set opacity of boxThree to 50
				set reflection showing of boxThree to true
				set reflection value of boxThree to 40

				set textBox to make new text item at end of text items with properties ¬
					{position:{100, 400}, width:400, height:100}
				set object text of textBox to "Ein Textfeld mit Größe"

				set rule to make new line at end of lines with properties ¬
					{start point:{100, 600}, end point:{500, 700}}

				set photo to make new image at end of images with properties ¬
					{file:POSIX file imageFile, position:{700, 420}, width:120, height:90}
				set description of photo to "ein Probebild"

				-- An image the app is then made to *replace*, which is the
				-- only way a script can produce a cropped one: `file name` is
				-- writable, and setting it to a picture of a different shape
				-- makes Keynote scale the new picture to fill the old frame
				-- and cut the overflow away with a mask. That mask is the
				-- non-destructive edit state this crate must refuse to write
				-- underneath.
				set swapped to make new image at end of images with properties ¬
					{file:POSIX file imageFile, position:{450, 620}, width:160, height:120}
				set file name of swapped to (POSIX file squareFile)

				set boxFour to make new shape at end of shapes with properties ¬
					{position:{800, 620}, width:150, height:120}
				set locked of boxFour to true
			end tell

			set answer to ((count of shapes of slide 1 of doc) as text) & " shapes, " & ¬
				((count of images of slide 1 of doc) as text) & " images, " & ¬
				((count of lines of slide 1 of doc) as text) & " lines, " & ¬
				((count of text items of slide 1 of doc) as text) & " text items"
			with timeout of 600 seconds
				save doc in POSIX file target
			end timeout
		on error message number code
			try
				close doc saving no
			end try
			error message number code
		end try
		close doc saving yes
	end tell
	return answer
end run
