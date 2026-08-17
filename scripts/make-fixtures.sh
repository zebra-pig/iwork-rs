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
	local script=$here/applescript/$name.applescript

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

printf 'building fixtures in %s\n' "$dir"

osa_warm pages
build pages-plain pages 120
build pages-styled pages 120
build pages-unicode pages 120
build pages-report pages 180

osa_warm numbers
build numbers-values numbers 180
build numbers-formats numbers 240
build numbers-large numbers 300 "$csv" "$(basename "$csv" .csv)"

osa_warm key
build keynote-deck key 240 "$png"

printf '\n%d built, %d left alone, %d failed\n' "$built" "$skipped" "$failed"
[ "$failed" = 0 ]
