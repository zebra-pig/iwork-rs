#!/bin/bash
#
# edit-and-save.sh — have Numbers change a cell of a document, and save.
#
#   scripts/edit-and-save.sh <document.numbers> <table> <cell> <value>
#
# The probe for anything the app *recalculates* rather than stores: a formula's
# cached value and a chart's grid are both caches of what the calculation engine
# last worked out. Making the change through the app and saving leaves those
# caches as the app wrote them, for a reader to check afterwards — which is how
# a written chart mediator is proved to be one the app believes.

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
	printf 'edit-and-save: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

# One caller at a time: `cargo test` drives the apps from several test binaries
# at once, and every one of them starts by closing what Numbers has open.
osa_acquire

osa_warm numbers || exit 1
osa_try numbers "$timeout" "$here/applescript/edit-and-save.applescript" \
	"$document" "$2" "$3" "$4"
status=$?
if [ "$status" -ne 0 ]; then
	printf 'edit-and-save: Numbers did not answer for %s (status %s)\n' "$document" "$status" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	osa_reset numbers
	exit "$status"
fi
answer=$OSA_STDOUT
osa_close numbers 60 >/dev/null 2>&1 || true
printf '%s\n' "$answer"
