-- A Pages document with a table and an image in it.
--
-- Not by inserting them: Pages refuses to create any drawable from a script.
-- `make new table`, `make new image`, `make new shape` and `make new text item`
-- all come back with "Don't know how to create TMAScript…InfoProxy", in
-- Pages 15.3.1, whatever properties are supplied. Only the document's body text
-- can be created from nothing.
--
-- What *is* scriptable is choosing the template a new document starts from, and
-- some templates ship with a table and a photo already placed. So the fixture
-- comes from a template, and the cells are then filled by script — which does
-- work, unlike creating the table that holds them.
--
-- Template names are localised, so several candidates are tried and the first
-- one that actually yields a table wins.

on run argv
	set target to item 1 of argv
	set candidates to {"Project Proposal", "Cyber Stark Checklist", "Business Modern Report", "Simple Report", "Blank"}

	tell application id "com.apple.Pages"
		set doc to missing value
		set chosen to ""
		repeat with candidate in candidates
			set found to missing value
			try
				set found to template (candidate as text)
			end try
			if found is not missing value then
				set doc to make new document with properties {document template:found}
				if (count of tables of doc) > 0 then
					set chosen to candidate as text
					exit repeat
				end if
				close doc saving no
				set doc to missing value
			end if
		end repeat
		if doc is missing value then
			error "none of the candidate templates has a table" number 8000
		end if

		try
			-- `rows` and `columns` are element names, not free variable names:
			-- `set rows to …` addresses the table's rows and fails.
			tell table 1 of doc
				set rowTotal to row count
				set columnTotal to column count
				if rowTotal ≥ 2 and columnTotal ≥ 2 then
					set value of cell 1 of row 2 to "Süßwaren"
					set value of cell 2 of row 2 to 1234
				end if
				if rowTotal ≥ 3 and columnTotal ≥ 2 then
					set value of cell 1 of row 3 to "Getränke"
					set value of cell 2 of row 3 to 567.89
				end if
			end tell
			set summary to chosen & ", " & (count of tables of doc) & " table(s), " & ¬
				(count of images of doc) & " image(s)"
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
	return summary
end run
