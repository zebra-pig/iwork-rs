#!/bin/bash
#
# recalculation.sh — does Numbers recalculate a formula this crate wrote?
#
#   scripts/recalculation.sh <document.numbers> <formula cell> <precedent cell> <value>
#
# Prints two tab-separated lines, as `applescript/recalculation.applescript`
# documents: what the formula cell shows when the document opens, and what it
# shows after the app itself has changed a cell the formula reads. The second
# line is the whole point — Numbers does not recalculate on open, so a formula
# the engine knows nothing about answers the first question and never the
# second. The document is closed without saving.

set -u

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=lib/osa.sh
. "$here/lib/osa.sh"

timeout=${IWORK_APP_TIMEOUT:-300}

if [ $# -ne 4 ]; then
	sed -n '3,5p' "$0" | sed 's/^# \{0,1\}//'
	exit 2
fi

document=$1
if [ ! -e "$document" ]; then
	printf 'recalculation: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

# One caller at a time: `cargo test` drives the apps from several test binaries
# at once, and every one of them starts by closing what Numbers has open.
osa_acquire

osa_warm numbers || exit 1
osa_try numbers "$timeout" "$here/applescript/recalculation.applescript" \
	"$document" "$2" "$3" "$4"
status=$?
if [ "$status" -ne 0 ]; then
	printf 'recalculation: Numbers did not answer for %s (status %s)\n' "$document" "$status" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	osa_reset numbers
	exit "$status"
fi
answer=$OSA_STDOUT
osa_close numbers 60 >/dev/null 2>&1 || true
printf '%s\n' "$answer"
