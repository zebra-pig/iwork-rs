#!/bin/bash
#
# section-oracle.sh — ask Pages what a document's sections are.
#
#   scripts/section-oracle.sh <document.pages> [output.tsv]
#
# Prints the TSV that `applescript/section-oracle.applescript` produces: the
# document-body flag, the number of sections, and each section's text with its
# length. That is the oracle `tests/pages.rs` measures the section decoder
# against — a section's range is computed from the body storage's table 17 and
# the app either agrees with the arithmetic, character for character, or does
# not.
#
# Same shape as `drawable-oracle.sh`, and for the same three reasons: the app
# is cleared first because it restores whatever it had open at the last quit,
# the lock is taken because `cargo test` drives Pages from several binaries at
# once, and the exit status is read *before* it is tested, because `$?` after
# `if ! cmd` is the status of the negation and reports success for every
# failure.

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
	printf 'section-oracle: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

case "$document" in
*.pages) ;;
*)
	printf 'section-oracle: sections are a Pages idea: %s\n' "$document" >&2
	exit 2
	;;
esac

osa_acquire
osa_warm pages || exit 1
osa_run "$timeout" "$here/applescript/section-oracle.applescript" "$document"
outcome=$?
if [ "$outcome" -ne 0 ]; then
	printf 'section-oracle: Pages did not answer for %s (status %s)\n' \
		"$document" "$outcome" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	osa_reset pages
	exit "$outcome"
fi
answer=$OSA_STDOUT
osa_close pages 60 >/dev/null 2>&1 || true

if [ $# -eq 2 ]; then
	printf '%s\n' "$answer" >"$2"
else
	printf '%s\n' "$answer"
fi
