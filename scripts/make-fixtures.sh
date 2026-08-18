#!/bin/bash
#
# make-fixtures.sh — build the test corpus with Pages, Numbers and Keynote.
#
#   scripts/make-fixtures.sh [--force] [--dir DIR] [NAME...]
#
# No iWork documents are committed to this repository — they are other people's
# files. This script is the substitute: given the three apps, it writes a corpus
# of documents that between them exercise the parts of the format this crate
# claims to understand, into tests/fixtures/generated (which is gitignored).
# Anyone with the apps can rebuild it; nobody has to be sent a file.
#
# The documents are written *by the apps*, which is the whole point. A fixture
# this crate produced would only prove the crate agrees with itself.
#
# Existing files are left alone unless --force is given, so re-running it after
# adding one fixture costs one document rather than seven. NAME filters by
# fixture name (`pages-styled`, `numbers-large`, …).
#
# Every call has a timeout, because osascript has none, and every builder closes
# its document even when it fails part way — an app left holding an unsaved
# document will put up a dialog at the next attempt and then answer nothing.
#
# What the apps would not do, probed against the dictionaries rather than
# assumed:
#
#   Pages will not create a table, an image, a shape or a text item from a
#   script ("Don't know how to create TMAScriptTableInfoProxy"), so the fixture
#   that has those comes from a template that ships with them.
#
#   Pages has no bold and no italic. Rich text carries `font`, `size` and
#   `color`; weight and slant are reached by naming a face.
#
#   Keynote has no master slides any more. Slides have a `base layout` and the
#   document has `slide layout` elements.

set -u

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
# shellcheck source=lib/osa.sh
. "$here/lib/osa.sh"

force=0
dir=$repo/tests/fixtures/generated
wanted=()

while [ $# -gt 0 ]; do
	case $1 in
	--force) force=1 ;;
	--dir)
		shift
		dir=$1
		;;
	-h | --help)
		sed -n '3,6p' "$0" | sed 's/^# \{0,1\}//'
		exit 2
		;;
	-*)
		printf 'make-fixtures: unknown option %s\n' "$1" >&2
		exit 2
		;;
	*) wanted+=("$1") ;;
	esac
	shift
done

mkdir -p "$dir"
assets=$(mktemp -d) || exit 70
trap 'rm -rf "$assets"' EXIT

# An 8×8 PNG, inline so the script needs nothing but a shell to run. Small on
# purpose: what the media tests care about is that the bytes come back
# unchanged, and eight rows of that is as convincing as eight hundred.
png=$assets/probe.png
base64 -d >"$png" <<'PNG'
iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAbElEQVR4nA3JQQEAMAgDMZSgpEqq
hOepQAlKqmjLN1VFFypcTLHFFSmqmm7UuJlmm2vSP0QLCYsRK05EP0wbGZsxa87EP4YeNHiYYYcb
Mj+WXrR4mWWXW7I/jj50+JhjjztyP0IHBYcJGy4kPLAsVIExyOP/AAAAAElFTkSuQmCC
PNG

# Two more, deliberately of different shapes: 32x24 and 64x64. What they are
# for is the media half of the drawables work — a picture whose replacement has
# a different aspect ratio is the case where the app crops and this crate warns,
# and a fixture cannot show that with one image.
wide=$assets/probe-wide.png
base64 -d >"$wide" <<'PNG'
iVBORw0KGgoAAAANSUhEUgAAACAAAAAYCAIAAAAUMWhjAAAAJElEQVR4nGO4o6FBU8QwasGo
BaMWjFowasGoBaMWjFowNCwAAEUJhC7iQqksAAAAAElFTkSuQmCC
PNG

square=$assets/probe-square.png
base64 -d >"$square" <<'PNG'
iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAAAeklEQVR4nO3PUQkAIBTAwJfE
JPbHWIbw4xAGC3Cbtc/XDRc0oAUNaEEDWtCAFjSgBQ1oQQNa0IAWNKAFDWhBA1rQgBY0oAUN
aEEDWtCAFjSgBQ1oQQNa0IAWNKAFDWhBA1rQgBY0oAUNaEEDWtCAFjSgBQ1oQQNa8NgFMB0h
Dwa7FBUAAAAASUVORK5CYII=
PNG

# 300 rows of plausible spreadsheet data. Numbers stores a table's cells in
# tiles, and 300 rows is past the point where one tile holds them all.
csv=$assets/large.csv
{
	printf 'Region,Quartal,Einheiten,Preis,Umsatz,Marge,Aktiv,Notiz\n'
	awk 'BEGIN {
		for (i = 1; i <= 300; i++)
			printf "R%d,Q%d,%d,%.2f,%.2f,%.2f,%s,Zeile %d\n",
				i % 7, i % 4 + 1, i * 3, i * 1.5, i * 4.5, (i % 17) / 100,
				(i % 2 ? "TRUE" : "FALSE"), i
	}'
} >"$csv"

built=0
skipped=0
failed=0

# build NAME EXTENSION TIMEOUT [extra arguments for the builder]
build() {
	local name=$1 extension=$2 limit=$3
	shift 3
	local target=$dir/$name.$extension
	local script=${script_override:-$here/applescript/$name.applescript}

	if [ ${#wanted[@]} -gt 0 ]; then
		local match=0 candidate
		for candidate in "${wanted[@]}"; do
			[ "$candidate" = "$name" ] && match=1
		done
		[ "$match" = 1 ] || return 0
	fi

	if [ -e "$target" ] && [ "$force" = 0 ]; then
		printf '  %-24s exists, left alone\n' "$name.$extension"
		skipped=$((skipped + 1))
		return 0
	fi
	rm -rf "$target"

	if osa_run "$limit" "$script" "$target" "$@"; then
		printf '  %-24s %s\n' "$name.$extension" "$OSA_STDOUT"
		built=$((built + 1))
		return 0
	fi

	# One retry, from a clean app. Most failures seen here were not the
	# script's: an app still holding a document from a previous failure
	# answers a save with -10000, or with nothing.
	printf '  %-24s retrying after a reset\n' "$name.$extension" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/      /' >&2
	rm -rf "$target"
	osa_reset "$extension"
	if osa_run "$limit" "$script" "$target" "$@"; then
		printf '  %-24s %s\n' "$name.$extension" "$OSA_STDOUT"
		built=$((built + 1))
		return 0
	fi

	printf '  %-24s FAILED\n' "$name.$extension" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/      /' >&2
	rm -rf "$target"
	osa_reset "$extension"
	failed=$((failed + 1))
	return 1
}

# build_template NAME TIMEOUT TEMPLATE-ID [sheet to delete]
#
# The organised-table fixtures come from templates Apple ships, because
# **nothing else can make them.** Numbers' scripting dictionary has no sort, no
# filter, no category, no pivot and no conditional-highlighting command, and the
# menu items that do have them need a document *window* — which needs an
# unlocked screen, which an unattended Mac does not have. Apple's own templates
# demonstrate exactly these features; creating a document from one makes Numbers
# 15.3.1 write the whole structure out again, which is what a fixture is for.
#
# The template is named by `id` — the path inside the bundle, the same on every
# Mac — and never by `name`, which is localised.
#
# The existence check afterwards is not paranoia. `close ... saving no` on a
# document that was just saved to a new location **deletes it**, silently and
# minutes later, and the builder's own script had to be taught to close with
# `saving yes` instead. A fixture that is not on disk when its builder said it
# was is worth failing over rather than discovering three steps downstream.
build_template() {
	local name=$1 limit=$2
	shift 2
	script_override=$here/applescript/from-template.applescript
	build "$name" numbers "$limit" "$@"
	local status=$?
	script_override=
	if [ "$status" = 0 ] && [ ! -e "$dir/$name.numbers" ]; then
		printf '  %-24s VANISHED after the app said it saved it\n' "$name.numbers" >&2
		failed=$((failed + 1))
		built=$((built - 1))
		return 1
	fi
	return $status
}

# copy_template NAME PATH-INSIDE-/Applications EXTENSION
#
# A fixture that is Apple's own bundle rather than a document an app wrote.
# Used exactly once, for the hyperlinks — see the note beside the call.
copy_template() {
	local name=$1 source=$2 extension=$3
	local target=$dir/$name.$extension

	if [ ${#wanted[@]} -gt 0 ]; then
		local match=0 candidate
		for candidate in "${wanted[@]}"; do
			[ "$candidate" = "$name" ] && match=1
		done
		[ "$match" = 1 ] || return 0
	fi
	if [ -e "$target" ] && [ "$force" = 0 ]; then
		printf '  %-24s exists, left alone\n' "$name.$extension"
		skipped=$((skipped + 1))
		return 0
	fi
	if [ ! -e "/Applications/$source" ]; then
		printf '  %-24s NOT FOUND: /Applications/%s\n' "$name.$extension" "$source" >&2
		failed=$((failed + 1))
		return 1
	fi
	cp "/Applications/$source" "$target"
	printf '  %-24s copied from the app bundle\n' "$name.$extension"
	built=$((built + 1))
}

printf 'building fixtures in %s\n' "$dir"

script_override=

# The apps are a single resource; take them for the whole build. The trap
# osa_acquire installs would replace the one above, so this one does both.
osa_acquire
trap 'osa_release; rm -rf "$assets"' EXIT INT TERM

osa_warm pages
build pages-plain pages 120
build pages-styled pages 120
build pages-unicode pages 120
build pages-report pages 180

# Lists, which no script can make: Pages' rich text carries `font`, `size` and
# `color` and nothing else, so a bullet, an indent level and a
# `TSWP.ListStyleArchive` in use have to come from a template. The Real Estate
# Flyer uses three named list styles in one storage — "None", "Bullet" and
# "Bullet 2" — changes indent level in the middle of the last of them, and has
# a layout break (`U+0005`) inside a paragraph for good measure.
script_override=$here/applescript/pages-from-template.applescript
build pages-lists pages 240 "Application/04_Real_Estate_Flyer/ISO"

# The document *structure* Pages is the only app to have, and none of which a
# script can create either: `make new section` is not in the dictionary,
# `delete section` answers -10000, and there is no header, footer, footnote,
# column or table-of-contents anywhere in it. All 640 bundled Pages templates
# were scanned with this crate; these four are what the interesting numbers
# picked out.
#
#   book        six sections and **facing pages** (`TP.SettingsArchive` field
#               34), which only the eighteen novel-shaped templates have
#   toc         the only table of contents in the whole install: two of the 640
#               templates carry a `TSWP.TOCInfoArchive`, and this is one
#   layout      a **page-layout** document — `TP.SettingsArchive.body` false,
#               which is exactly the 388 templates that also carry a
#               `TP.PageTemplateArchive` — with a linked-text-box thread
#               (`TSWP.FlowInfoArchive`, 19 templates have one) and the only
#               header and footer text in this corpus
#   numbering   a section that restarts its page numbers at 2
#               (`section_page_number_kind` 1, `section_page_number_start` 2),
#               one that hides its header and footer on the first page, and a
#               section background fill
build pages-book pages 300 "Application/11B_Novel_Modern/ISO"
build pages-toc pages 300 "Application/00C_Textbook_Portrait/ISO"
build pages-layout pages 300 "Application/08_Journal_Newsletter/ISO"
build pages-numbering pages 300 "Application/65_Sales_Bold_Report_PM/ISO"
script_override=

# Columns, which cannot be built the same way. Of the 640 templates exactly
# **one** lays its body out in more than one column — `02_ResearchPaper_JP`,
# which has both an equal two-column layout and a non-equal one — and Pages on
# this machine does not offer it: `every template whose id is
# "Application/02_ResearchPaper_JP/ISO"` comes back empty, because the
# Japanese templates are not in this locale's list. So this fixture is the
# bundle itself renamed, as `numbers-links` is; a `.template` is the same ZIP a
# `.pages` is, and Pages opens the copy and reads its text back.
copy_template pages-columns \
	"Pages Creator Studio.app/Contents/SharedSupport/Templates/02_ResearchPaper_JP/ISO.template" \
	pages

osa_warm numbers
build numbers-values numbers 180
build numbers-formats numbers 240
build numbers-large numbers 300 "$csv" "$(basename "$csv" .csv)"

# Everything a table is *organised* by. One template per feature cluster:
#
#   categories  a two-level category on a text column, with a SUM summary row
#               and the group tree Numbers keeps out of line
#   pivot       two pivot tables over one source — one with rows, columns and a
#               summed value, one deliberately left empty
#   rules       a filter set with a real rule, rows the filter hid, columns the
#               user hid, conditional-highlighting rules and custom cell formats
#   sorted      a sort rule on one column
#
# `Stocks` also carries live stock-quote cells, which ground rule 8 says this
# crate may carry and read but must never author — a fixture that proves the
# carrying is worth having.
build_template numbers-categories 240 "Application/21_BasicCategories/Traditional"
build_template numbers-pivot 300 "Application/21_Pivot_Table_Basics/Traditional"
build_template numbers-rules 300 "Application/26_Stocks/Traditional"
build_template numbers-sorted 240 "Application/44_Notetaking_Colorful_Log_PM/Traditional"

# The only hyperlinks in the whole install, and the one fixture no app can be
# made to write.
#
# All 901 bundled templates were scanned for `TSWP.HyperlinkFieldArchive`
# (2032): **five objects in three Numbers templates**, and none at all in the
# 640 Pages templates or the 182 Keynote themes. Then every route to making one
# was tried and every one failed. No app's scripting dictionary has a link
# command — `sdef` over all three returns nothing but `sourceURL`. Setting a
# Pages body text to a sentence containing `https://example.com` and an e-mail
# address does not auto-link it. And **instantiating any of the three templates
# strips the links**: Numbers writes the document out with the text and without
# the smart fields, so `numbers-links` built the ordinary way had none.
#
# So this fixture is Apple's template bundle itself, renamed. A `.nmbtemplate`
# is the same ZIP a `.numbers` is, Numbers opens the copy and reads its text
# back through `app-check.sh`, and it carries what nothing else here can: two
# links in one storage, one `mailto:` and one `http:`, one of them terminated
# by an entry with no field and one running to the end of the text with no
# terminator at all — the two shapes a smart-field run comes in.
copy_template numbers-links \
	"Numbers Creator Studio.app/Contents/SharedSupport/Templates/46_Business_Modern_Invoice_PM/Traditional.nmbtemplate" \
	numbers

osa_warm key
build keynote-deck key 240 "$png"

# One of every drawable a script can put on a slide, with the properties the
# app can read back — geometry, rotation, opacity, reflection, lock — plus an
# image whose file the app replaces, which is how a *cropped* image gets into
# the corpus. Keynote is the only app that will create a drawable from a
# script, and TSD is cross-app, so this deck is the shape fixture for all three.
build keynote-shapes key 300 "$wide" "$square"

printf '\n%d built, %d left alone, %d failed\n' "$built" "$skipped" "$failed"
[ "$failed" = 0 ]
