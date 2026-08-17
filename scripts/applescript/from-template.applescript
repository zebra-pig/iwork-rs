-- Build a fixture from one of the templates Numbers ships with.
--
-- Sort rules, filters, categories, conditional highlighting, custom cell
-- formats and pivot tables have one thing in common: **not one of them can be
-- created from a script.** Numbers' dictionary has no sort, no filter, no
-- category and no pivot command, and the menu items that do them need a window
-- to act on. Driving the UI needs an unlocked screen, which an unattended
-- machine does not have.
--
-- Apple's own templates do have them. `Categories`, `Pivot Table Basics`,
-- `My Stocks` and `Note Taking Colourful Log` are demonstrations of exactly
-- these features, and creating a document from one and saving it makes
-- **Numbers 15.3.1 write the whole structure out again** — which is what a
-- fixture is for. The document is the app's, not this repository's.
--
-- Templates are addressed by `id`, never by `name`: the id is the path inside
-- the bundle (`Application/21_BasicCategories/Traditional`) and is the same on
-- every Mac, while the name is localised — "Categories" here, "Kategorien" on
-- a German system.
--
--   osascript from-template.applescript OUT.numbers TEMPLATE-ID [SHEET-NAME]
--
-- The optional third argument names a sheet to delete before saving, which is
-- how the "how to use this template" sheets are kept out of the corpus.

on run argv
	set target to item 1 of argv
	set templateID to item 2 of argv

	tell application id "com.apple.Numbers"
		set candidates to (every template whose id is templateID)
		if candidates is {} then
			error "no template with id " & templateID number 8000
		end if
		set doc to make new document with properties {document template:item 1 of candidates}
		try
			if (count of argv) > 2 then
				set doomed to item 3 of argv
				try
					delete (first sheet of doc whose name is doomed)
				end try
			end if
			set answer to (count of sheets of doc) as text
			with timeout of 600 seconds
				save doc in POSIX file target
			end timeout
		on error message number code
			try
				close doc saving no
			end try
			error message number code
		end try
		-- **`saving yes`, and it matters.** A document made from a template and
		-- saved to a new location is still, to the app, an editing session that
		-- has not been committed: `close doc saving no` on one does not answer
		-- for minutes and then **deletes the file that was just written**.
		-- Three fixtures were lost that way, each time to something else
		-- closing the app's documents later. `saving yes` returns at once and
		-- leaves the document on disk. The document is already saved, so this
		-- writes nothing new.
		with timeout of 120 seconds
			close doc saving yes
		end timeout
	end tell
	return "from " & templateID & ", " & answer & " sheet(s)"
end run
