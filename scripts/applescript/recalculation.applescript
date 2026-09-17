-- Does the app *recalculate* a formula this crate wrote?
--
-- The question the table oracle cannot answer. Numbers does not recalculate
-- when a document opens: the value in the cell record is what it draws, and it
-- goes on drawing it — so a formula the calculation engine knows nothing about
-- looks exactly like one it knows, until something the formula reads changes.
-- This changes one.
--
--   recalculation.applescript <document> <formula cell> <precedent cell> <value>
--
-- Output, tab separated, two lines:
--
--   on-open     <the formula cell's value>   <its formula>
--   after-edit  <the formula cell's value>   <the precedent's value>
--
-- The document is closed **without saving**: the point is what the app
-- computes, not what it would write.

on run argv
	set target to item 1 of argv
	set formulaCell to item 2 of argv
	set precedentCell to item 3 of argv
	set newValue to (item 4 of argv) as number

	set wanted to my basename(target)
	set alsoWanted to my stem(wanted)
	do shell script "open -g -b com.apple.Numbers " & quoted form of target

	-- Wait for *this* document: Numbers restores its last session, and
	-- `document 1` is somebody else's until the right one arrives.
	set doc to missing value
	repeat 60 times
		tell application id "com.apple.Numbers"
			try
				repeat with d in documents
					if (name of d) is wanted or (name of d) is alsoWanted then
						set doc to d
						exit repeat
					end if
				end repeat
			end try
		end tell
		if doc is not missing value then exit repeat
		delay 1
	end repeat
	if doc is missing value then error "Numbers did not open " & target number 8010

	set harvest to {}
	with timeout of 300 seconds
		tell application id "com.apple.Numbers"
			set t to table 1 of sheet 1 of doc
			set end of harvest to "on-open" & tab & ¬
				(value of cell formulaCell of t as text) & tab & ¬
				(formula of cell formulaCell of t as text)
			set value of cell precedentCell of t to newValue
			-- The recalculation is not synchronous with the event that caused
			-- it; a second is generous for a table this small.
			delay 2
			set end of harvest to "after-edit" & tab & ¬
				(value of cell formulaCell of t as text) & tab & ¬
				(value of cell precedentCell of t as text)
			close doc without saving
		end tell
	end timeout
	return my join(harvest)
end run

on basename(p)
	set text item delimiters to "/"
	set n to last text item of p
	set text item delimiters to ""
	return n
end basename

on stem(n)
	set text item delimiters to "."
	set parts to text items of n
	if (count of parts) > 1 then set parts to items 1 thru -2 of parts
	set s to parts as text
	set text item delimiters to ""
	return s
end stem

on join(items_)
	set text item delimiters to linefeed
	set s to items_ as text
	set text item delimiters to ""
	return s
end join
