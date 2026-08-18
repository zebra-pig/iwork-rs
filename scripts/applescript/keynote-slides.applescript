-- A deck built for the *show* rather than for the drawables: many layouts,
-- notes almost everywhere, two skipped slides, slide numbers turned on, and a
-- transition on every slide that has one to give.
--
-- `keynote-deck` already covers "a slide with a picture on it". What it does
-- not cover is the inventory phase 8a reads: a deck where the layout varies
-- slide by slide, where a skipped slide sits in the middle *and* at the end,
-- where the slide-number placeholder is actually drawn, and where the
-- transitions are not all "none".
--
-- `transition properties` is a record and has to be set whole — setting
-- `transition effect of transition properties of s` is not a settable
-- reference and answers -10006. The effect constants only resolve inside the
-- `tell application` block, the same rule the chart types follow.
--
-- A text item cannot be *made* with its text: `make new text item … with
-- properties {object text:"…"}` answers -10000, the same "AppleEvent handler
-- failed" a `make` with any unsettable-at-creation property gives. The text is
-- set on the item afterwards, which is what the shapes fixture does too, and
-- the item has to be made inside a `tell slide` block rather than addressed as
-- `text items of slide 3 of doc`.
--
-- `skipped` is set after the slide exists: passing it to `make new slide with
-- properties` is accepted and then ignored. Every slide is made before any of
-- them is filled in, because interleaving creation with content is what lost
-- the document reference in the charts fixture.

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
			set slide numbers showing of doc to true

			set titleLayout to my layoutFor("Title", 1, doc)
			set bullets to my layoutFor("Title & Bullets", 4, doc)
			set statement to my layoutFor("Statement", 12, doc)
			set titleOnly to my layoutFor("Title Only", 10, doc)
			set sectionLayout to my layoutFor("Section", 9, doc)
			set quoteLayout to my layoutFor("Quote", 14, doc)
			set blankLayout to my layoutFor("Blank", 17, doc)

			-- Every slide first, then every slide's contents.
			repeat with wanted in {bullets, sectionLayout, bullets, statement, quoteLayout, titleOnly, blankLayout}
				make new slide at end of slides of doc with properties {base layout:contents of wanted}
			end repeat

			set titles to {"Der Aufbau des Vortrags", "Erstens: die Zahlen", ¬
				"Zwischenstand", "Zweitens: die Umlaute", "Ein Zitat", ¬
				"Diese Folie fehlt", "Ende ohne Titel"}
			set bodies to {"Aufbau" & return & "Zahlen" & return & "Umlaute", "", ¬
				"Größe" & return & "Straße" & return & "Fuß", "", "", "", ""}
			set notes to {"Begrüßung, dann Überblick.", "Langsam sprechen.", ¬
				"Hier die Tabelle zeigen.", "Umlaute betonen: ä ö ü.", ¬
				"Das Zitat vorlesen.", "Diese Folie wird übersprungen.", ""}

			tell slide 1 of doc
				set object text of default title item to "Ein Vortrag über Folien"
				set object text of default body item to ¬
					"Sieben Folien" & return & "Fünf Layouts" & return & "Zwei übersprungen"
				set presenter notes to "Die erste Folie trägt den Titel."
			end tell

			repeat with i from 2 to 8
				set s to slide i of doc
				try
					set object text of default title item of s to item (i - 1) of titles
				end try
				if (item (i - 1) of bodies) is not "" then
					try
						set object text of default body item of s to item (i - 1) of bodies
					end try
				end if
				set presenter notes of s to item (i - 1) of notes
			end repeat

			-- A text item that is not a placeholder, so the role table has one
			-- of each on the same slide.
			tell slide 3 of doc
				set freeBox to make new text item at end of text items with properties ¬
					{position:{200, 800}, width:600, height:100}
				set object text of freeBox to "Eine freie Textbox"
			end tell

			-- One skipped slide in the middle and one at the end. The app
			-- numbers the rest around them and answers -1 for these.
			set skipped of slide 7 of doc to true
			set skipped of slide 8 of doc to true

			set transition properties of slide 2 of doc to ¬
				{transition effect:dissolve, transition duration:1.5, automatic transition:false}
			set transition properties of slide 3 of doc to ¬
				{transition effect:push, transition duration:2.0, automatic transition:true, transition delay:3.0}
			set transition properties of slide 4 of doc to ¬
				{transition effect:magic move, transition duration:1.0, automatic transition:false}
			set transition properties of slide 5 of doc to ¬
				{transition effect:wipe, transition duration:0.75, automatic transition:false}
			set transition properties of slide 6 of doc to ¬
				{transition effect:confetti, transition duration:2.5, automatic transition:false}

			set answer to ((count of slides of doc) as text) & " slides, skipped=" & ¬
				(skipped of slide 7 of doc as text) & "/" & ¬
				(skipped of slide 8 of doc as text) & ", numbers=" & ¬
				(slide numbers showing of doc as text)
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
