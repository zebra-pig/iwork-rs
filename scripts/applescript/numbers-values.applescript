-- Two sheets, three tables, and one cell of every type Numbers stores.
--
-- The point of the fixture is the *cell storage*: text goes through a string
-- table, numbers and booleans and durations do not, dates are their own thing
-- again, and a formula cell carries both an expression and the value the app
-- last computed for it. Numbers evaluates the formulas as it writes them, so
-- what this document holds in C3 is 84 — which makes the app an oracle: a
-- reader that disagrees with it is wrong.
--
-- The date is assembled from `current date` rather than written as a date
-- literal, because a literal is parsed in the machine's locale and the same
-- script would fail on a Mac set to a different one.

on run argv
	set target to item 1 of argv

	set stamp to current date
	set year of stamp to 2024
	set month of stamp to March
	set day of stamp to 1
	set time of stamp to 9 * hours + 30 * minutes

	tell application id "com.apple.Numbers"
		set doc to make new document
		try
			tell sheet 1 of doc
				set name to "Werte"
				tell table 1
					set name to "Zellarten"
					set row count to 10
					set column count to 5
					set value of cell "A1" to "Art"
					set value of cell "B1" to "Wert"
					set value of cell "C1" to "Abgeleitet"

					set value of cell "A2" to "Text"
					set value of cell "B2" to "Zeichenkette mit Umlaut: Größe"

					set value of cell "A3" to "Zahl"
					set value of cell "B3" to 42
					set value of cell "C3" to "=B3*2"

					set value of cell "A4" to "Wahrheitswert"
					set value of cell "B4" to true
					set value of cell "C4" to "=NOT(B4)"

					set value of cell "A5" to "Datum"
					set value of cell "B5" to stamp

					set value of cell "A6" to "Dauer"
					set value of cell "B6" to "1h 30m"

					set value of cell "A7" to "Dezimalzahl"
					set value of cell "B7" to 3.14159
					set value of cell "C7" to "=ROUND(B7,2)"

					set value of cell "A8" to "Summe"
					set value of cell "C8" to "=SUM(B3,B7)"

					-- Away from the header row and column; Numbers refuses to
					-- merge a range that straddles the header boundary.
					merge range "D9:E9"
					set value of cell "D9" to "verbunden"
				end tell

				set extra to make new table with properties {row count:4, column count:3}
				tell extra
					set name to "Zweite Tabelle"
					set value of cell "A1" to "Posten"
					set value of cell "B1" to "Menge"
					set value of cell "A2" to "Schrauben"
					set value of cell "B2" to 250
					set value of cell "A3" to "Muttern"
					set value of cell "B3" to 500
					set value of cell "B4" to "=SUM(B2:B3)"
				end tell
			end tell

			-- `make new sheet at end of sheets of doc` is accepted by the
			-- parser and then fails with -10000. The plain form works.
			tell doc to set other to make new sheet
			tell other
				set name to "Zweites Blatt"
				tell table 1
					set name to "Kennzahlen"
					set value of cell "A1" to "Kennzahl"
					set value of cell "B1" to "Wert"
					set value of cell "A2" to "Umsatz"
					set value of cell "B2" to 10000
					set value of cell "A3" to "Kosten"
					set value of cell "B3" to 7500
					set value of cell "A4" to "Gewinn"
					set value of cell "B4" to "=B2-B3"
					-- A reference that leaves the table it is written in.
					set value of cell "B5" to "=Zellarten::B3"
				end tell
			end tell

			set answer to "C3=" & (value of cell "C3" of table "Zellarten" of sheet 1 of doc as text) & ¬
				" B5=" & (value of cell "B5" of table "Kennzahlen" of sheet 2 of doc as text)
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
