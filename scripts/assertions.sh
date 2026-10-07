#!/bin/bash
#
# assertions.sh — what does the app complain about, to itself, opening this?
#
#   scripts/assertions.sh <document>
#
# The apps open a great deal that is wrong and repair it without a word to the
# user. They do say it to the unified log: every broken invariant is a line in
# category TSUAssertCat, naming the class, the source file and what was nil,
# missing or out of bounds. This runs app-check.sh on the document and prints
# those lines for the time it took, grouped and counted, identifiers folded.
#
# It is how Keynote 15.4's "couldn't read the file" was read: 77 "is not
# strongly referenced from message" before the first nil. A document the app
# wrote itself opens with none.
#
# `log` is a builtin in zsh, which prints nothing and exits 0 for all of this;
# hence /usr/bin/log.
#
# Exit codes: 0 opened with no assertion, 1 opened with some, otherwise
# app-check.sh's own.

set -u
here=$(cd "$(dirname "$0")" && pwd)
[ $# -eq 1 ] || { sed -n '3,5p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

case "$1" in
*.key | *.kth) process=Keynote ;;
*.numbers | *.nmbtemplate) process=Numbers ;;
*.pages | *.template) process=Pages ;;
*) echo "assertions.sh: not an iWork document: $1" >&2; exit 2 ;;
esac

start=$(date '+%Y-%m-%d %H:%M:%S')
"$here/app-check.sh" "$1" >/dev/null
opened=$?

found=$(/usr/bin/log show --start "$start" --info --debug --style compact \
	--predicate "process == \"$process\" AND category == \"TSUAssertCat\"" 2>/dev/null |
	grep -E 'Assertion failure #[0-9]+: [-+]\[' |
	sed -E 's/^.*Assertion failure #[0-9]+: //; s/-[0-9]+\]/-N]/g; s#/Library/Caches[^ ]*/##' |
	sort | uniq -c | sort -rn)

[ $opened -eq 0 ] || { echo "$found"; exit $opened; }
[ -z "$found" ] && exit 0
echo "$found"
exit 1
