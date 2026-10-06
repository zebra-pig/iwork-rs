#!/bin/bash
#
# cell-look-oracle.sh — ask Numbers how it draws a document's cells.
#
#   scripts/cell-look-oracle.sh <document.numbers> [output.tsv]
#
# Prints the TSV that `applescript/cell-look-oracle.applescript` produces: a
# line per table and a line per cell of its first rows, with the background
# colour, font name, font size and text colour the app reports. That is the
# oracle table styling is measured against. `table-oracle.sh` says what a cell
# holds; this says what it looks like — and a fill can be in the archive, pass
# `iwork check` and survive the app's own save without being what the cell is
# painted with.
#
# Same shape as `table-oracle.sh`, and for the same reasons: the app is cleared
# first because it restores whatever it had open at the last quit, the lock is
# taken because `cargo test` drives Numbers from several binaries at once, and
# the exit status is read *before* it is tested.

set -u

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=lib/osa.sh
. "$here/lib/osa.sh"

timeout=${IWORK_APP_TIMEOUT:-300}

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
	sed -n '3,5p' "$0" | sed 's/^# \{0,1\}//'
	exit 2
fi

document=$1
if [ ! -e "$document" ]; then
	printf 'cell-look-oracle: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

osa_acquire
osa_warm numbers || exit 1
osa_try numbers "$timeout" "$here/applescript/cell-look-oracle.applescript" "$document"
status=$?
if [ "$status" -ne 0 ]; then
	printf 'cell-look-oracle: Numbers did not answer for %s (status %s)\n' "$document" "$status" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	osa_reset numbers
	exit "$status"
fi
answer=$OSA_STDOUT
osa_close numbers 60 >/dev/null 2>&1 || true

if [ $# -eq 2 ]; then
	printf '%s\n' "$answer" >"$2"
else
	printf '%s\n' "$answer"
fi
