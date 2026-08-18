-- A chart zoo built from known numbers — the oracle for the chart decoder.
--
-- There is no way to read a chart's data back out of any of the three apps:
-- `chart` is in all three dictionaries and carries nothing but the geometry it
-- inherits from `iWork item`. So the oracle has to be the *input*. Keynote is
-- the only app with a chart-creating command — `add chart`, with row names,
-- column names, a grid of numbers, a type and a grouping — and a chart built
-- that way carries exactly those strings and exactly those doubles in its
-- `TSCH.ChartGridArchive`. A decoder that prints them back is a decoder that
-- reads the grid.
--
-- Each chart gets its own hundred: chart `i` holds `i×100 + 1, 2, 3` in its
-- first row and `+11, 12, 13` in its second, so no two charts share a value and
-- a mis-ordered read cannot pass.
--
-- `add chart`'s `type` parameter is Keynote's *legacy chart type* enumeration,
-- seventeen constants that predate the twenty-eight-value `TSCH.ChartType` the
-- file carries. All seventeen work, and the mapping between them was read off
-- the documents this script writes:
--
--   pie_2d 5 · vertical_bar_2d 1 · stacked_vertical_bar_2d 6 ·
--   horizontal_bar_2d 2 · stacked_horizontal_bar_2d 7 · pie_3d 16 ·
--   vertical_bar_3d 12 · stacked_vertical_bar_3d 17 · horizontal_bar_3d 13 ·
--   stacked_horizontal_bar_3d 18 · area_2d 4 · stacked_area_2d 8 · line_2d 3 ·
--   line_3d 14 · area_3d 15 · stacked_area_3d 19 · scatterplot_2d 9
--
-- "vertical bar" is a *column* chart and "horizontal bar" is a bar chart, which
-- is the one place the two vocabularies disagree about a word.
--
-- Two more slides carry what the type sweep cannot: a chart grouped **by
-- column** rather than by row, which is the other value of
-- `ChartArchive.series_direction` and swaps which axis of the grid is a series;
-- and a chart with a hole in its data, because a blank chart cell is a
-- *present but empty* `GridValue` and a decoder that reads it as 0.0 invents a
-- data point.
--
-- Three AppleScript traps, all found here.
--
--   The chart-type constants only resolve **inside** the `tell application`
--   block — outside it, `pie_2d` is an undefined variable.
--
--   Every slide has to exist before the charts go on. Interleaving `make new
--   slide` with `add chart` made Keynote lose the document reference entirely
--   (-1728, "Can't get document id …") halfway through the run.
--
--   **`add chart` ignores the slide it is given.** Its direct parameter is
--   documented as "the slide to add the chart to", and whether it is passed as
--   `add chart (slide i of doc) …` or implied by a `tell slide i of doc` block,
--   every chart lands on the document's **current slide** — eighteen of them
--   stacked on one, in the first version of this fixture. Setting `current
--   slide of doc` before each call is what actually places them.

on run argv
	set target to item 1 of argv
	set report to ""

	tell application id "com.apple.Keynote"
		set kinds to {pie_2d, vertical_bar_2d, stacked_vertical_bar_2d, horizontal_bar_2d, ¬
			stacked_horizontal_bar_2d, pie_3d, vertical_bar_3d, stacked_vertical_bar_3d, ¬
			horizontal_bar_3d, stacked_horizontal_bar_3d, area_2d, stacked_area_2d, ¬
			line_2d, line_3d, area_3d, stacked_area_3d, scatterplot_2d}
		set extras to 2
		set wanted to (count of kinds) + extras

		set doc to make new document
		try
			repeat with i from 2 to wanted
				make new slide at end of slides of doc
			end repeat

			repeat with i from 1 to count of kinds
				set base to i * 100
				try
					set current slide of doc to slide i of doc
					add chart (slide i of doc) row names {"Reihe A", "Reihe B"} ¬
						column names {"Q1", "Q2", "Q3"} ¬
						data {{base + 1, base + 2, base + 3}, ¬
							{base + 11, base + 12, base + 13}} ¬
						type (item i of kinds) group by chart row
				on error message number code
					set report to report & (i as text) & ":ERR" & (code as text) & " "
				end try
			end repeat

			-- Series by column: the same grid read the other way round, so the
			-- three quarters are the series and the two rows the categories.
			try
				set current slide of doc to slide ((count of kinds) + 1) of doc
				add chart (slide ((count of kinds) + 1) of doc) ¬
					row names {"Nord", "Süd"} ¬
					column names {"Jan", "Feb", "Mär"} ¬
					data {{7001, 7002, 7003}, {7011, 7012, 7013}} ¬
					type vertical_bar_2d group by chart column
			on error message number code
				set report to report & "bycolumn:ERR" & (code as text) & " "
			end try

			-- A hole in the data. `missing value` is AppleScript's null and the
			-- grid's blank is a zero-length `GridValue`; if the app refuses it
			-- the slide simply stays empty and the fixture says so.
			try
				set current slide of doc to slide ((count of kinds) + 2) of doc
				add chart (slide ((count of kinds) + 2) of doc) ¬
					row names {"Lücke"} ¬
					column names {"A", "B", "C"} ¬
					data {{8001, missing value, 8003}} ¬
					type vertical_bar_2d group by chart row
			on error message number code
				set report to report & "blank:ERR" & (code as text) & " "
			end try

			set report to report & (count of slides of doc) & " slides"
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
	return report
end run
