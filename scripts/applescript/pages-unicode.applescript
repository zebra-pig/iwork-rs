-- Text that is not Latin-1, so that anything indexing it has to be honest.
--
-- iWork counts text in UTF-16 code units, and every character class below
-- disagrees with a different naive count:
--
--   Umlauts and ß      one code unit, two UTF-8 bytes
--   CJK                one code unit, three UTF-8 bytes
--   Emoji              *two* code units — a surrogate pair — and one glyph
--   Flag, ZWJ sequence several code points, still one glyph
--   e + U+0301         two code points that render as one é
--
-- A range that is right in characters, bytes or glyphs but wrong in code units
-- will land in the middle of a surrogate pair here and nowhere else.

on run argv
	set target to item 1 of argv
	tell application id "com.apple.Pages"
		set d to make new document
		try
			set body text of d to "Grüße aus Zürich — Größe, Maß, Straße." & return & ¬
				"日本語のテキストと中文文本。" & return & ¬
				"Emoji: 🎬 👩‍💻 🇨🇭 — und ein kombiniertes é." & return & ¬
				"Mixed: Ω≈ç√∫˜µ≤≥÷ 𝄞 𝕬"
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
	return "umlauts, CJK, emoji, combining marks"
end run
