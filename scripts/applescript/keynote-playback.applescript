-- A deck whose playback settings are all off their defaults.
--
-- The four the dictionary exposes — `auto loop`, `auto play`, `auto restart`
-- and `maximum idle duration` — are the only ones a script can move, and each
-- one is a single field of `KN.ShowArchive`. Every other deck in this corpus has
-- them at their defaults, so this fixture is the other half of an A/B: whatever
-- differs in the show archive between this deck and `keynote-slides` is these
-- four settings and nothing else.
--
-- What is deliberately *not* here, because nothing can put it here:
--
--   the presentation type (`mode`, field 9 — normal / links only / self
--   playing) has no dictionary vocabulary at all;
--   the two self-playing delays (fields 10 and 11) likewise;
--   the soundtrack (field 17) — `audio clip` is an element of a *slide*, not
--   the document, and there is no soundtrack term anywhere in the sdef;
--   a recording (field 7) — Play > Record Slideshow is menu-only.
--
-- `maximum idle duration` is typed `integer` in the dictionary and lands in a
-- `double` field, which is worth a fixture on its own: 137 is not a round
-- number of minutes and not the 900-second default, so a reader that guessed
-- the units or the wire type would disagree.

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
			set bullets to my layoutFor("Title & Bullets", 4, doc)
			make new slide at end of slides of doc with properties {base layout:bullets}

			tell slide 1 of doc
				set object text of default title item to "Selbstlaufend"
			end tell

			set auto loop of doc to true
			set auto play of doc to true
			set auto restart of doc to true
			set maximum idle duration of doc to 137

			set answer to "loop=" & (auto loop of doc as text) & ¬
				", play=" & (auto play of doc as text) & ¬
				", restart=" & (auto restart of doc as text) & ¬
				", idle=" & (maximum idle duration of doc as text)
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
