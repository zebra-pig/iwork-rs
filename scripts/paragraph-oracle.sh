#!/bin/bash
#
# paragraph-oracle.sh — ask Pages how it draws each paragraph of the body.
#
#   scripts/paragraph-oracle.sh <document.pages> [output.tsv]
#
# Prints one line per body paragraph: the size Pages draws it at, the font
# and the colour (three 16-bit channels), tab separated. That is the oracle text styling is measured against. A style can
# be structurally right, pass `iwork check` and survive the app's own save,
# and still not be what the paragraph is drawn with — the archive says what
# was asked for, and only the app says what was done.
#
# Same shape as `section-oracle.sh`, and for the same three reasons: the app
# is cleared first because it restores whatever it had open at the last quit,
# the lock is taken because `cargo test` drives Pages from several binaries at
# once, and the exit status is read *before* it is tested.

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
	printf 'paragraph-oracle: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

case "$document" in
*.pages) ;;
*)
	printf 'paragraph-oracle: body paragraphs are a Pages idea: %s\n' "$document" >&2
	exit 2
	;;
esac

osa_acquire
osa_warm pages || exit 1
osa_try pages "$timeout" "$here/applescript/paragraph-oracle.applescript" "$document"
outcome=$?
if [ "$outcome" -ne 0 ]; then
	printf 'paragraph-oracle: Pages did not answer for %s (status %s)\n' \
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
