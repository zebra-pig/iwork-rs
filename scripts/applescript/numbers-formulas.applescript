-- A formula for every node the calculation engine has, written by Numbers.
--
-- AppleScript will not create a category, a filter or a chart, but it *will*
-- put any string into a cell — and a string beginning with `=` is a formula.
-- That makes the whole of `TSCE` reachable from a script: this fixture is a
-- zoo of one formula per AST node type, operator, reference shape and literal
-- kind, and the app is the oracle for every one of them, because `formula of
-- cell` reports the text back in the app's own spelling.
--
-- Two things the fixture proves that no static document could:
--
--   * a formula Numbers *rejects* is stored as text and reports no formula at
--     all, so the answer this script returns names every rejected case rather
--     than leaving a silent hole in the corpus;
--   * the table `Alt` is renamed to `Neu` **after** the formula that points at
--     it is written, so a decoder that resolves cross-table references by name
--     string reads the wrong table and one that resolves by identity does not.
--
-- Formulas go in column C of `Zoo`, one per row, with the case's name in A and
-- a number in B for the relative references to reach.

on run argv
	set target to item 1 of argv

	-- {label, formula}. Order is the order of §Formulas in FORMAT.md: the
	-- arithmetic operators first, then comparisons, text, references,
	-- functions, control flow, lookup, dates and durations, errors and the
	-- shapes that have no operator at all.
	set cases to {¬
		{"addition", "=B2+1"}, ¬
		{"subtraktion", "=B3-1"}, ¬
		{"multiplikation", "=B4*2"}, ¬
		{"division", "=B5/2"}, ¬
		{"potenz", "=B6^2"}, ¬
		{"negation", "=-B7"}, ¬
		{"vorzeichen", "=+B8"}, ¬
		{"prozent", "=B9*10%"}, ¬
		{"prozentliteral", "=50%"}, ¬
		{"klammern", "=(B11+1)*2"}, ¬
		{"groesser", "=B12>1"}, ¬
		{"groesser-gleich", "=B13>=1"}, ¬
		{"kleiner", "=B14<1"}, ¬
		{"kleiner-gleich", "=B15<=1"}, ¬
		{"gleich", "=B16=1"}, ¬
		{"ungleich", "=B17<>1"}, ¬
		{"verkettung", "=\"a\"&\"b\""}, ¬
		{"verkettung-zelle", "=A19&\"!\""}, ¬
		{"zeichenkette-anfuehrung", "=\"Er sagte \"\"hallo\"\"\""}, ¬
		{"laenge", "=LEN(\"hello\")"}, ¬
		{"gross", "=UPPER(\"abc\")"}, ¬
		{"links", "=LEFT(\"abcdef\",3)"}, ¬
		{"verketten", "=CONCATENATE(\"x\",\"y\",\"z\")"}, ¬
		{"relativ", "=B25"}, ¬
		{"absolut-beide", "=$B$2"}, ¬
		{"absolut-zeile", "=B$2"}, ¬
		{"absolut-spalte", "=$B2"}, ¬
		{"bereich", "=SUM(B2:B4)"}, ¬
		{"bereich-absolut", "=SUM($B$2:$B$4)"}, ¬
		{"bereich-gemischt", "=SUM($B2:B$4)"}, ¬
		{"ganze-spalte", "=SUM(B)"}, ¬
		{"ganze-spalte-buchstabe", "=COUNT(B:B)"}, ¬
		{"ganze-zeile", "=SUM(2:2)"}, ¬
		{"kreuztabelle", "=Daten::B2"}, ¬
		{"kreuztabelle-bereich", "=SUM(Daten::B2:B4)"}, ¬
		{"kreuzblatt", "=Fern::A2"}, ¬
		{"umbenannt", "=Alt::A1"}, ¬
		{"kopfname", "=SUM(Daten::Menge)"}, ¬
		{"funktion-ohne-argument", "=PI()"}, ¬
		{"funktion-ein-argument", "=ABS(-5)"}, ¬
		{"funktion-viele-argumente", "=SUM(1,2,3,4,5)"}, ¬
		{"funktion-verschachtelt", "=ROUND(SQRT(ABS(B43))*2,3)"}, ¬
		{"wahr-token", "=TRUE"}, ¬
		{"wahr-funktion", "=TRUE()"}, ¬
		{"falsch-token", "=FALSE"}, ¬
		{"wenn", "=IF(B47>0,\"ja\",\"nein\")"}, ¬
		{"wenn-und", "=IF(AND(B48>0,B48<100),1,0)"}, ¬
		{"wenn-oder", "=IF(OR(B49>0,FALSE),1,0)"}, ¬
		{"nicht", "=NOT(B50>0)"}, ¬
		{"sverweis", "=VLOOKUP(\"Muttern\",Daten::A1:C4,2,FALSE)"}, ¬
		{"index", "=INDEX(Daten::A1:C4,3,2)"}, ¬
		{"vergleich", "=MATCH(\"Muttern\",Daten::A1:A4,0)"}, ¬
		{"datum-funktion", "=DATE(2024,3,1)"}, ¬
		{"datum-jahr", "=YEAR(DATE(2024,3,1))"}, ¬
		{"dauer", "=DURATION(0,1,2,30)"}, ¬
		{"dauer-in-stunden", "=DUR2HOURS(DURATION(0,1,2,30))"}, ¬
		{"fehler-division", "=1/0"}, ¬
		{"fehler-abgefangen", "=IFERROR(1/0,\"kaputt\")"}, ¬
		{"leerzeichen", "= B60 + 1"}, ¬
		{"zahl-dezimal", "=0.1+0.2"}, ¬
		{"zahl-gross", "=1000000*3"}, ¬
		{"zahl-klein", "=0.00001*2"}, ¬
		{"array", "={1,2;3,4}"}, ¬
		{"liste", "=SUM((B2:B3,B5:B6))"}, ¬
		{"leeres-argument", "=SUM(B2,,B3)"}, ¬
		{"let", "=LET(x,2,x*3)"}, ¬
		{"lambda", "=LAMBDA.APPLY(LAMBDA(x,x+1),2)"}, ¬
		{"lambda-byrow", "=SUM(BYROW(Daten::B2:B4,LAMBDA(r,SUM(r))))"}, ¬
		{"reduce", "=REDUCE(0,Daten::B2:B4,LAMBDA(a,b,a+b))"}, ¬
		{"sequenz", "=SEQUENCE(1,3)"}, ¬
		{"ueberlauf", "=SUM(C71#)"}, ¬
		{"schnittmenge", "=SUM(INTERSECT.RANGES(B2:B10,B5:B15))"}, ¬
		{"vereinigung", "=SUM(UNION.RANGES(B2,B4))"}, ¬
		{"mehrdeutiger-kopfname", "=SUM(Daten2::Menge)"}, ¬
		{"kopflose-tabelle", "=Kopflos::B2"}, ¬
		{"zwei-kopfzeilen", "=Doppelkopf::B3"}, ¬
		{"referenzfehler", "=Weg::B1"}, ¬
		{"kopfzelle", "=B1"}, ¬
		{"dauer-summe", "=DUR2DAYS(DURATION(1)+DURATION(2))"}, ¬
		{"datumswert", "=DATEVALUE(\"2024-03-01\")"}, ¬
		{"ersetzen", "=SUBSTITUTE(\"aaa\",\"a\",\"b\")"}, ¬
		{"text-vor", "=TEXTBEFORE(\"a-b\",\"-\")"}, ¬
		{"schalter", "=SWITCH(B84,1,\"eins\",2,\"zwei\",\"sonst\")"}, ¬
		{"xverweis", "=XLOOKUP(\"Muttern\",Daten::A1:A4,Daten::B1:B4)"}, ¬
		{"zaehlenwenn", "=COUNTIF(Daten::B1:B4,\">100\")"}, ¬
		{"wenns", "=IFS(B87>100,\"gross\",TRUE,\"klein\")"}, ¬
		{"let-mehrfach", "=LET(x,2,y,3,x*y)"}, ¬
		{"kopfname-operator", "=Daten3::B2"}, ¬
		{"kopfname-leerzeichen", "=Daten3::C2"}, ¬
		{"kopfname-apostroph", "=Daten3::D2"}, ¬
		{"kopfname-klammern", "=Daten3::E2"}, ¬
		{"kopfname-funktionsname", "=Daten3::F2"}, ¬
		{"kopfname-zeile-apostroph", "=Daten3::B3"}, ¬
		{"kopfname-ganze-spalte", "=SUM(Daten3::C)"}, ¬
		{"kopflos-bereich", "=SUM(Kopflos::A1:B2)"}}

	tell application id "com.apple.Numbers"
		set doc to make new document
		try
			tell sheet 1 of doc
				set name to "Formeln"
				tell table 1
					set name to "Zoo"
					-- Six columns, not three: a formula that spills — the `#`
					-- operator's other half — needs empty cells to spill into.
					set column count to 6
					set row count to (count of cases) + 1
					set value of cell "A1" to "Fall"
					set value of cell "B1" to "Wert"
					set value of cell "C1" to "Formel"
				end tell

				-- The data a lookup looks things up in, and the header names
				-- a name reference resolves against.
				set daten to make new table with properties {row count:4, column count:4}
				tell daten
					set name to "Daten"
					set value of cell "A1" to "Posten"
					set value of cell "B1" to "Menge"
					set value of cell "C1" to "Preis"
					set value of cell "A2" to "Schrauben"
					set value of cell "B2" to 250
					set value of cell "C2" to 0.1
					set value of cell "A3" to "Muttern"
					set value of cell "B3" to 500
					set value of cell "C3" to 0.05
					set value of cell "A4" to "Naegel"
					set value of cell "B4" to 125
					set value of cell "C4" to 0.2
					-- A header name used inside the table that owns it.
					set value of cell "D1" to "Gesamt"
					set value of cell "D2" to "=SUM(Menge)"
				end tell
			end tell

			tell doc to set fern to make new sheet
			tell fern
				set name to "Fernblatt"
				tell table 1
					set name to "Fern"
					set value of cell "A1" to "Fern A1"
					set value of cell "A2" to 99
				end tell
				set alt to make new table with properties {row count:3, column count:2}
				tell alt
					set name to "Alt"
					set value of cell "A1" to 7
				end tell

				-- A second table with the *same* column header as `Daten`, so
				-- the name `Menge` is no longer unique in the document and the
				-- app has to say which table it means.
				set zweite to make new table with properties {row count:3, column count:2}
				tell zweite
					set name to "Daten2"
					set value of cell "A1" to "Posten"
					set value of cell "B1" to "Menge"
					set value of cell "A2" to "Bolzen"
					set value of cell "B2" to 11
					set value of cell "A3" to "Scheiben"
					set value of cell "B3" to 22
				end tell

				-- No headers at all: nothing to name a cell with, so every
				-- reference into it must come out in A1 notation.
				set nackt to make new table with properties {row count:3, column count:3}
				tell nackt
					set name to "Kopflos"
					set header row count to 0
					set header column count to 0
					set value of cell "B2" to 5
				end tell

				-- Two header rows and one header column: which of the two
				-- rows names a column, and whether the two are joined, is
				-- only answerable from a document that has both.
				set doppelt to make new table with properties {row count:4, column count:3}
				tell doppelt
					set name to "Doppelkopf"
					set header row count to 2
					set value of cell "A1" to "Kopf1"
					set value of cell "B1" to "Oben"
					set value of cell "A2" to "Kopf2"
					set value of cell "B2" to "Unten"
					set value of cell "A3" to "Zeile3"
					set value of cell "B3" to 5
				end tell

				-- Header names that need quoting, and one that does not:
				-- which characters make the app wrap a name in single quotes
				-- is only answerable from names that contain them.
				set heikel to make new table with properties {row count:3, column count:6}
				tell heikel
					set name to "Daten3"
					set value of cell "B1" to "A+B"
					set value of cell "C1" to "x y"
					set value of cell "D1" to "it's"
					set value of cell "E1" to "Preis (netto)"
					set value of cell "F1" to "SUM"
					set value of cell "A2" to "normal"
					set value of cell "A3" to "mit'Hoch"
					set value of cell "B2" to 1
					set value of cell "C2" to 2
					set value of cell "D2" to 3
					set value of cell "E2" to 4
					set value of cell "F2" to 5
					set value of cell "B3" to 6
				end tell

				-- The column a formula points at, removed after the formula
				-- is written: the only way to make a stored `#REF!` here.
				set weg to make new table with properties {row count:3, column count:3}
				tell weg
					set name to "Weg"
					set value of cell "B1" to 5
				end tell
			end tell

			-- The formulas themselves, and a number beside each one for the
			-- relative references to reach.
			tell table "Zoo" of sheet "Formeln" of doc
				repeat with i from 1 to count of cases
					set c to item i of cases
					set r to i + 1
					set value of cell ("A" & r) to item 1 of c
					set value of cell ("B" & r) to i
					set value of cell ("C" & r) to item 2 of c
				end repeat
			end tell

			-- The identity proof: rename the table the `umbenannt` case
			-- points at, *after* its formula is written. A file that stored
			-- the name would now point at nothing.
			set name of table "Alt" of sheet "Fernblatt" of doc to "Neu"

			-- …and the column the `referenzfehler` case points at, removed
			-- the same way round: the formula is written first, so what is
			-- left behind is a real stored reference error.
			tell table "Weg" of sheet "Fernblatt" of doc to remove column 2

			-- Which cases the app refused. A formula it cannot parse is kept
			-- as text and reports no formula, and a fixture with a silent
			-- hole in it is worse than one that says where the hole is.
			set refused to {}
			tell table "Zoo" of sheet "Formeln" of doc
				repeat with i from 1 to count of cases
					set r to i + 1
					if (formula of cell ("C" & r)) is missing value then
						set end of refused to item 1 of (item i of cases)
					end if
				end repeat
			end tell

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

	set AppleScript's text item delimiters to " "
	if (count of refused) is 0 then
		set answer to ((count of cases) as text) & " formulas, none refused"
	else
		set answer to ((count of cases) as text) & " formulas, refused: " & (refused as text)
	end if
	set AppleScript's text item delimiters to ""
	return answer
end run
