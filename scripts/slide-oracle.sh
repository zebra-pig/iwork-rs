#!/bin/bash
#
# slide-oracle.sh — ask Keynote what is in a deck.
#
#   scripts/slide-oracle.sh <document.key> [output.tsv]
#
# Prints the TSV that `applescript/slide-oracle.applescript` produces: the show
# line, one line per slide layout and one line per slide with its number, its
# base layout, its skipped flag, its title, its body, its presenter notes and
# its transition. That is the oracle this repository's Keynote reader is
# measured against — see `tests/keynote.rs`, which runs it and compares every
# field it can.
#
# Same shape as `drawable-oracle.sh`, and for the same reasons: one caller at a
# time because several test binaries drive the apps at once, and the exit status
# is taken *before* it is tested, because `$?` after `if ! cmd` is the status of
# the negation and reports success for every failure.

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
	printf 'slide-oracle: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

case ${document##*.} in
key | KEY | Key) ;;
*)
	printf 'slide-oracle: slides are a Keynote idea: %s\n' "$document" >&2
	exit 2
	;;
esac

osa_acquire

osa_warm key || exit 1
osa_try key "$timeout" "$here/applescript/slide-oracle.applescript" "$document"
outcome=$?
if [ "$outcome" -ne 0 ]; then
	printf 'slide-oracle: Keynote did not answer for %s (status %s)\n' \
		"$document" "$outcome" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	osa_reset key
	exit "$outcome"
fi
# Keep the answer before anything else calls osa_run — osa_close is one, and it
# would overwrite OSA_STDOUT with its own silence.
answer=$OSA_STDOUT
osa_close key 60 >/dev/null 2>&1 || true

if [ $# -eq 2 ]; then
	printf '%s\n' "$answer" >"$2"
else
	printf '%s\n' "$answer"
fi
