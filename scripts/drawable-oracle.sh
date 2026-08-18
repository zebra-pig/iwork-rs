#!/bin/bash
#
# drawable-oracle.sh — ask the app what is placed in a document.
#
#   scripts/drawable-oracle.sh <document> [output.tsv]
#
# Prints the TSV that `applescript/drawable-oracle.applescript` produces: one
# line per container (slide, page, sheet) and one per placed object, with its
# class, position, size, lock, rotation and opacity. That is the oracle this
# repository's drawable decoder is measured against — see `tests/drawables.rs`,
# which runs it and compares every rectangle.
#
# Same shape as `table-oracle.sh`, and for the same reasons: the app is cleared
# first because it restores whatever it had open at the last quit, and the exit
# status is taken *before* it is tested, because `$?` after `if ! cmd` is the
# status of the negation and reports success for every failure.

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
	printf 'drawable-oracle: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

extension=$(printf '%s' "${document##*.}" | tr '[:upper:]' '[:lower:]')
osa_bundle "$extension" >/dev/null || exit 2

# One caller at a time: `cargo test` drives the apps from several test binaries
# at once, and every one of them starts by closing what the app has open.
osa_acquire

osa_warm "$extension" || exit 1
osa_try "$extension" "$timeout" "$here/applescript/drawable-oracle.applescript" "$document"
outcome=$?
if [ "$outcome" -ne 0 ]; then
	printf 'drawable-oracle: %s did not answer for %s (status %s)\n' \
		"$extension" "$document" "$outcome" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	osa_reset "$extension"
	exit "$outcome"
fi
# Keep the answer before anything else calls osa_run — osa_close is one, and it
# would overwrite OSA_STDOUT with its own silence.
answer=$OSA_STDOUT
osa_close "$extension" 60 >/dev/null 2>&1 || true

if [ $# -eq 2 ]; then
	printf '%s\n' "$answer" >"$2"
else
	printf '%s\n' "$answer"
fi
