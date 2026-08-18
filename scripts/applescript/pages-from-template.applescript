-- Build a Pages fixture from one of the templates Pages ships with.
--
-- The Numbers twin of this script (`from-template.applescript`) exists because
-- no script can create a sort rule or a pivot table. This one exists for the
-- same reason and about text: **Pages' dictionary cannot make a list.** Rich
-- text carries `font`, `size` and `color` and nothing else — no bullet, no
-- indent level, no list style — so a document with a real
-- `TSWP.ListStyleArchive` in use, and with the per-paragraph level that goes
-- with it, has to come from somewhere else.
--
-- Apple's templates have them. `04_Real_Estate_Flyer` uses three named list
-- styles in one storage and changes indent level in the middle of the last
-- one, and creating a document from it makes Pages 15.3.1 write the whole
-- structure out again.
--
-- Templates are addressed by `id` — the path inside the bundle, the same on
-- every Mac — never by the localised `name`. Pages exposes the `ISO` variant of
-- each template on this machine, so the ids read
-- `Application/04_Real_Estate_Flyer/ISO`.
--
--   osascript pages-from-template.applescript OUT.pages TEMPLATE-ID
--
-- `close … saving yes`, for the reason the Numbers script records: a document
-- made from a template and saved to a new location is still an uncommitted
-- editing session, and `saving no` on one does not answer for minutes and then
-- deletes the file that was just written.

on run argv
	set target to item 1 of argv
	set templateID to item 2 of argv

	tell application id "com.apple.Pages"
		set candidates to (every template whose id is templateID)
		if candidates is {} then
			error "no template with id " & templateID number 8000
		end if
		set doc to make new document with properties {document template:item 1 of candidates}
		try
			with timeout of 600 seconds
				save doc in POSIX file target
			end timeout
		on error message number code
			try
				close doc saving no
			end try
			error message number code
		end try
		with timeout of 120 seconds
			close doc saving yes
		end timeout
	end tell
	return "from " & templateID
end run
