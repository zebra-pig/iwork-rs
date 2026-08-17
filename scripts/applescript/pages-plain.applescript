-- The smallest document Pages will write: one paragraph, no styling of its own.
--
-- Useful precisely because it is boring. Anything this crate cannot do to this
-- document it cannot do to any of them.

on run argv
	set target to item 1 of argv
	tell application id "com.apple.Pages"
		set d to make new document
		try
			set body text of d to "Ein einziger Absatz, geschrieben von make-fixtures.sh."
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
	return "one paragraph"
end run
