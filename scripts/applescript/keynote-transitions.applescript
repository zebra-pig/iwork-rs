-- One slide per transition effect, and nothing else different between them.
--
-- This is the A/B deck phase 8b's transition work is measured against. The
-- dictionary reads only four things back — effect, duration, delay, automatic —
-- so every other parameter of a transition has to be *diffed* between slides
-- rather than asked for. That only says something if the slides are otherwise
-- identical, which is why every slide here is blank: no title, no body, no text,
-- the same layout, the same duration, the same delay, the same automatic flag.
-- Whatever differs in `KN.TransitionAttributesArchive` between slide i and slide
-- j is then the effect and nothing but.
--
-- The 44 constants are the whole of the app's `transition effects` enumeration,
-- in the order `sdef` lists it, and the cocoa string beside each one in the
-- dictionary is what lands in `AnimationAttributesArchive.effect`. They are
-- written out literally because an AppleScript enumerator cannot be built from a
-- string; a list of constants can be indexed, and that compiles.
--
-- The last two slides are the control: `wipe` again, at a different duration,
-- delay and automatic flag. Their `custom_*` block must match slide 25's, which
-- is what proves the block belongs to the effect rather than to the timing.
--
-- `transition properties` is a record and has to be set whole — setting
-- `transition effect of transition properties of s` is not a settable reference
-- and answers -10006. The constants only resolve inside the `tell application`
-- block.

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
			set fx to {no transition effect, magic move, shimmer, sparkle, swing, ¬
				object cube, object flip, object pop, object push, object revolve, ¬
				object zoom, perspective, clothesline, confetti, dissolve, drop, ¬
				droplet, fade through color, grid, iris, move in, push, reveal, ¬
				switch, wipe, blinds, color planes, cube, doorway, fall, flip, flop, ¬
				mosaic, page flip, pivot, reflection, revolving door, scale, swap, ¬
				swoosh, twirl, twist, fade and move, radial wipe}
			set effectCount to count of fx

			set blankLayout to my layoutFor("Blank", 17, doc)
			set base layout of slide 1 of doc to blankLayout
			repeat with i from 2 to (effectCount + 2)
				make new slide at end of slides of doc with properties ¬
					{base layout:blankLayout}
			end repeat

			repeat with i from 1 to effectCount
				set transition properties of slide i of doc to ¬
					{transition effect:item i of fx, transition duration:1.0, ¬
						transition delay:0.0, automatic transition:false}
			end repeat

			-- The control pair: the same effect at other timings.
			set transition properties of slide (effectCount + 1) of doc to ¬
				{transition effect:wipe, transition duration:3.25, ¬
					transition delay:2.5, automatic transition:true}
			set transition properties of slide (effectCount + 2) of doc to ¬
				{transition effect:magic move, transition duration:0.5, ¬
					transition delay:0.0, automatic transition:true}

			set answer to ((count of slides of doc) as text) & " slides, " & ¬
				(effectCount as text) & " effects"
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
