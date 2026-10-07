#!/bin/bash
#
# word-oracle.sh — ask Pages or Keynote how it draws each word.
#
#   scripts/word-oracle.sh <document.pages|document.key> [output.tsv]
#
# Prints one line per word — of the body for Pages, of every text item of
# every slide for Keynote: the word, its size, its font and its colour (three
# 16-bit channels), tab separated. The oracle for a run with a look of its own
# inside a paragraph, as paragraph-oracle.sh is for whole paragraphs.

set -u

here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib/osa.sh"

timeout=${IWORK_APP_TIMEOUT:-300}

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
	sed -n '3,5p' "$0" | sed 's/^# \{0,1\}//'
	exit 2
fi

document=$1
if [ ! -e "$document" ]; then
	printf 'word-oracle: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

case "$document" in
*.pages) app=pages ;;
*.key) app=key ;;
*)
	printf 'word-oracle: not a Pages or Keynote document: %s\n' "$document" >&2
	exit 2
	;;
esac

osa_acquire
osa_warm "$app" || exit 1
osa_try "$app" "$timeout" "$here/applescript/word-oracle.applescript" "$document"
outcome=$?
if [ "$outcome" -ne 0 ]; then
	printf 'word-oracle: the app did not answer for %s (status %s)\n' \
		"$document" "$outcome" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	osa_reset "$app"
	exit "$outcome"
fi
answer=$OSA_STDOUT
osa_close "$app" 60 >/dev/null 2>&1 || true

if [ $# -eq 2 ]; then
	printf '%s\n' "$answer" >"$2"
else
	printf '%s\n' "$answer"
fi
