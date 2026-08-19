-- A two-slide base deck for the european-correspondent example to write into.
--
-- The example authors a deck; it does not invent one. A theme is hundreds of
-- objects of stylesheet and a slide layout cannot be made from nothing, so the
-- base comes from Keynote — which is ground rule 3 (copy, don't synthesise)
-- applied to a whole document.
--
-- Two slides, because the deck needs two kinds: the title slide the theme opens
-- with, and one "Title & Bullets" slide the example copies once per remaining
-- slide. A copy wears its source's layout — Keynote's dictionary makes `slide
-- layout` read-only — so the bullets slide is the one that decides how the deck
-- looks.
--
-- Every placeholder is given text here, and both slides a presenter note, for a
-- reason that is not cosmetic: a placeholder the slide does not own has no
-- storage to write into, and a slide with no note has no notes storage either.
-- Typing into them is what makes the slide own them, and nothing in iwork-rs
-- can add one afterwards. The words are thrown away by the example.
--
-- Layout names come from the theme and are localised, so the lookup falls back
-- to an index, exactly as keynote-deck.applescript does.

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

	tell application id "com.apple.Keynote"
		set doc to make new document
		try
			tell slide 1 of doc
				set object text of default title item to "Title"
				try
					set object text of default body item to "Subtitle"
				end try
				set presenter notes to "Notes."
			end tell

			set bullets to my layoutFor("Title & Bullets", 4, doc)
			set bulletSlide to make new slide at end of slides of doc with properties {base layout:bullets}
			tell bulletSlide
				set object text of default title item to "Title"
				set object text of default body item to "First" & return & "Second" & return & "Third"
				set presenter notes to "Notes."
			end tell

			set answer to ((count of slides of doc) as text) & " slides on " & ¬
				(name of doc) & ", bullets layout " & (name of bullets)
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
