#!/bin/bash
#
# resave.sh — have the app open a document and write it out again.
#
#   scripts/resave.sh <document>
#
# For the parts of a document no scripting dictionary can read back. Pages has
# no header, footer, footnote or column property at all, so `app-check.sh` can
# only say the document opened. This says more: the app loaded the edit into
# its own model and wrote it out from there, and what is on disk afterwards was
# written by Pages, not by this crate. A header this crate invented badly does
# not survive that.
#
# The document is edited **in place**, which is the point — the caller works on
# a copy and decodes it afterwards.

set -u

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=lib/osa.sh
. "$here/lib/osa.sh"

timeout=${IWORK_APP_TIMEOUT:-300}

if [ $# -ne 1 ]; then
	sed -n '3,5p' "$0" | sed 's/^# \{0,1\}//'
	exit 2
fi

document=$1
if [ ! -e "$document" ]; then
	printf 'resave: no such document: %s\n' "$document" >&2
	exit 2
fi
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

extension=$(printf '%s' "${document##*.}" | tr '[:upper:]' '[:lower:]')
osa_bundle "$extension" >/dev/null || exit 2

osa_acquire
osa_warm "$extension" || exit 1
osa_run "$timeout" "$here/applescript/resave.applescript" "$document"
outcome=$?
if [ "$outcome" -ne 0 ]; then
	printf 'resave: the app would not open or save %s (status %s)\n' "$document" "$outcome" >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	# An app that refused a document is showing an alert, and an alert will not
	# answer a request to close anything.
	osa_kill "$extension"
	exit "$outcome"
fi
printf '%s\n' "$OSA_STDOUT"
