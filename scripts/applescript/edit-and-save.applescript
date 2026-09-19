-- Have Numbers itself change a cell, and save.
--
--   edit-and-save.applescript <document> <table> <cell> <value>
--
-- The probe for anything that is *recalculated* rather than stored: a formula's
-- value, and a chart's grid, are caches of what the calculation engine last
-- worked out, and the app rewrites them when something they depend on changes.
-- So this makes the change through the app, saves, and leaves the reading to
-- the caller — which can then open the file and see what the app wrote.
--
-- Prints the cell's value as the app reports it after the edit.

on run argv
	set target to item 1 of argv
	set tableName to item 2 of argv
	set cellName to item 3 of argv
	set newValue to (item 4 of argv) as number
	set wanted to do shell script "basename " & quoted form of target
	set stem to do shell script "basename " & quoted form of target & " .numbers"
	do shell script "open -g -b com.apple.Numbers " & quoted form of target
	set doc to missing value
	repeat 60 times
		tell application id "com.apple.Numbers"
			try
				repeat with d in documents
					if (name of d) is wanted or (name of d) is stem then
						set doc to d
						exit repeat
					end if
				end repeat
			end try
		end tell
		if doc is not missing value then exit repeat
		delay 1
	end repeat
	if doc is missing value then error "not opened" number 8010
	set answer to "not found"
	with timeout of 300 seconds
		tell application id "com.apple.Numbers"
			repeat with s in sheets of doc
				repeat with t in tables of s
					if (name of t) is tableName then
						set value of cell cellName of t to newValue
						delay 2
						set answer to (value of cell cellName of t as text)
						exit repeat
					end if
				end repeat
			end repeat
			save doc
			delay 3
			close doc
		end tell
	end timeout
	return answer
end run
