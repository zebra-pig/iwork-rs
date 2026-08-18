#!/bin/bash
#
# table-oracle.sh — ask Numbers what is in a document's tables.
#
#   scripts/table-oracle.sh <document.numbers> [output.tsv]
#
# Prints the TSV that `applescript/table-oracle.applescript` produces: one line
# per sheet, table, row, column and cell, with each cell's class, value,
# formatted value, data format and formula. That is the oracle this repository's
# table decoder is measured against — see `tests/tables.rs`, which runs this and
# compares every cell.
#
# The app is cleared first. Numbers restores whatever it had open at the last
# quit, and a script that opens a document and then reads `document 1` will
# happily read the *other* one — which looks like a decoder disagreeing with the
# app about a document it has never seen. Observed, and the reason this wrapper
# exists rather than a bare osascript call.

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
	printf 'table-oracle: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

# One caller at a time: `cargo test` drives the apps from three test binaries
# at once, and every one of them starts by closing what Numbers has open.
osa_acquire

osa_warm numbers || exit 1
# `$?` after `if ! cmd` is the *negated* status, so reading it there reported
# success for every failure and this script exited 0 with nothing to say. A
# caller then saw an empty answer from a healthy-looking run, which is the one
# failure mode a harness must never have. Take the status before testing it.
osa_try "$extension" "$timeout" "$here/applescript/table-oracle.applescript" "$document"
status=$?
if [ "$status" -ne 0 ]; then
	printf 'table-oracle: Numbers did not answer for %s (status %s)\n' "$document" "$status" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	osa_reset numbers
	exit "$status"
fi
# Keep the answer before anything else calls osa_run — `osa_close` is one, and
# it would overwrite OSA_STDOUT with its own silence.
answer=$OSA_STDOUT
osa_close numbers 60 >/dev/null 2>&1 || true

if [ $# -eq 2 ]; then
	printf '%s\n' "$answer" >"$2"
else
	printf '%s\n' "$answer"
fi
