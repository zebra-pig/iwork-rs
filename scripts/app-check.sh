#!/bin/bash
#
# app-check.sh — does the app that owns this document actually open it?
#
#   scripts/app-check.sh <document> [expected-substring]
#   scripts/app-check.sh --self-test <document>
#
# The tests in this repository prove the bytes are structurally sound and
# survive an independent decode. They cannot prove Pages will accept the result,
# and documents that passed every one of them have crashed the app on opening.
# This is the check that closes that gap, so every phase that writes something
# ends here.
#
# What it does: starts the app that owns the extension, clears whatever it was
# holding, hands it the document, reads back everything the app will tell it —
# body text, every cell of every table, every slide's title, body and presenter
# notes — optionally looks for a string in that, and closes without saving.
#
# What failure looks like, all of it observed rather than imagined:
#
#   The app refuses the document      No document appears. The app puts up an
#                                     alert instead, which no script can click,
#                                     so what comes back is error 8010, "Pages
#                                     did not open …", after a minute of
#                                     watching. Exit 1 — and the app is killed
#                                     rather than trusted, because the alert is
#                                     still there.
#   The app hangs                     No answer at all. The osascript call is
#                                     killed at the timeout, and so is the app.
#                                     Exit 124.
#   The app opens it but the edit is  Exit 3. This is the quiet failure worth
#   not there                         fearing — the document loads, looks fine,
#                                     and simply does not contain what was
#                                     written into it.
#
# Exit codes: 0 accepted (and the expected text found, if one was given),
# 1 refused, 2 usage, 3 expected text missing, 4 self-test failed, 124 timeout.
#
# --self-test takes a document the app is known to accept, corrupts a copy of it
# — one `Index/*.iwa` stream is replaced by random bytes of the same length,
# leaving a package that is still a valid ZIP with every entry present and
# stored — and checks that the good one passes and the corrupt one does not. Run
# it whenever you doubt this script is actually looking. Pages, Numbers and
# Keynote have each been watched refusing their own corrupted document this way.

set -u

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=lib/osa.sh
. "$here/lib/osa.sh"

timeout=${IWORK_APP_TIMEOUT:-120}

usage() {
	sed -n '3,6p' "$0" | sed 's/^# \{0,1\}//'
	exit 2
}

# check DOCUMENT [EXPECTED] — the whole of the job, so --self-test can call it.
check() {
	local document=$1 expected=${2-}
	local extension script bundle harvest

	if [ ! -e "$document" ]; then
		printf 'app-check: no such document: %s\n' "$document" >&2
		return 2
	fi
	document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

	extension=$(printf '%s' "${document##*.}" | tr '[:upper:]' '[:lower:]')
	bundle=$(osa_bundle "$extension") || return 2
	case "$extension" in
	pages) script=$here/applescript/check-pages.applescript ;;
	numbers) script=$here/applescript/check-numbers.applescript ;;
	key) script=$here/applescript/check-keynote.applescript ;;
	esac

	# Wait for the apps to be ours. `cargo test` runs its test binaries in
	# parallel and three of them come through here, so without the lock one
	# run's `osa_warm` closes another run's document — and "REFUSED" is
	# entirely the wrong thing to say about that.
	osa_acquire

	# Start the app and clear anything it is holding first. An app that is
	# still opening, or still showing the document from the last check, takes
	# long enough over the next `open` to trip the timeout — and "REFUSED" is
	# the wrong thing to say about a document nobody has looked at yet.
	osa_warm "$extension"

	# Two attempts, from a killed app the second time.
	#
	# A timeout here means "no answer in $timeout seconds", and there are two
	# reasons for that: the app is showing a dialog about a document it will
	# not open, which is what this script is for; or the app is simply busy,
	# which on a machine running six test binaries at once it repeatedly was —
	# a different fixture each run, each of them opening perfectly well on its
	# own. Reporting the second as the first is exactly the failure the README
	# warns about, in the direction that wastes an afternoon. A document the
	# app genuinely refuses is refused twice; a busy app is not busy from a
	# cold start. `make-fixtures.sh` has retried for the same reason since
	# Phase 1b.
	local attempt outcome=0
	for attempt in 1 2; do
		osa_run "$timeout" "$script" "$document"
		outcome=$?
		[ "$outcome" -eq 0 ] && break
		if [ "$attempt" = 1 ]; then
			printf 'app-check: no answer for %s — killing %s and trying once more\n' \
				"$(basename "$document")" "$bundle" >&2
			osa_kill "$extension"
			osa_warm "$extension"
		fi
	done
	case $outcome in
	0) ;;
	124)
		printf 'REFUSED (timeout): %s\n' "$document" >&2
		printf '  %s never answered, twice. A modal dialog is the usual reason.\n' "$bundle" >&2
		osa_kill "$extension"
		return 124
		;;
	*)
		printf 'REFUSED: %s\n' "$document" >&2
		printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
		# Not osa_reset: an app that has just refused a document is showing
		# an alert about it, and an alert is exactly what will not answer a
		# request to close anything.
		osa_kill "$extension"
		return 1
		;;
	esac

	harvest=$OSA_STDOUT
	if [ -n "$expected" ]; then
		if ! printf '%s' "$harvest" | grep -qF -- "$expected"; then
			printf 'OPENED but %s does not contain %s\n' "$document" "$expected" >&2
			printf '%s\n' "$harvest" | sed 's/^/  /' | head -40 >&2
			return 3
		fi
		printf 'ok: %s opened in %s and contains %s\n' \
			"$(basename "$document")" "$bundle" "$expected"
		return 0
	fi

	printf 'ok: %s opened in %s (%s lines of text read back)\n' \
		"$(basename "$document")" "$bundle" "$(printf '%s\n' "$harvest" | wc -l | tr -d ' ')"
	return 0
}

# Corrupt a copy of a document by replacing one object stream with noise.
#
# The copy stays a well-formed ZIP with every entry present and stored, so what
# the app rejects is the document, not the container.
corrupt() {
	local original=$1 copy=$2 entry work
	cp "$original" "$copy" || return 1
	entry=$(unzip -Z1 "$copy" | grep -E '^Index/.*\.iwa$' | head -1)
	if [ -z "$entry" ]; then
		printf 'app-check: %s has no Index/*.iwa to corrupt\n' "$original" >&2
		return 1
	fi
	work=$(mktemp -d) || return 1
	mkdir -p "$work/$(dirname "$entry")"
	unzip -p "$copy" "$entry" | wc -c | tr -d ' ' >"$work/size"
	head -c "$(cat "$work/size")" /dev/urandom >"$work/$entry"
	(cd "$work" && zip -q -X -0 "$copy" "$entry") || return 1
	rm -rf "$work"
	printf '%s' "$entry"
}

self_test() {
	local good=$1 bad status entry
	bad=$(mktemp -d)/corrupt.${good##*.}
	mkdir -p "$(dirname "$bad")"
	entry=$(corrupt "$good" "$bad") || return 4

	printf '== self-test: the good document ==\n'
	if ! check "$good"; then
		printf 'self-test failed: %s should be accepted\n' "$good" >&2
		return 4
	fi

	printf '== self-test: the same document with %s replaced by noise ==\n' "$entry"
	check "$bad"
	status=$?
	rm -rf "$(dirname "$bad")"
	if [ "$status" -eq 0 ]; then
		printf 'self-test failed: a corrupt document was accepted — this script is not looking\n' >&2
		return 4
	fi
	printf 'self-test passed: the corrupt copy was rejected (exit %d)\n' "$status"
	return 0
}

case "${1-}" in
'' | -h | --help) usage ;;
--self-test)
	[ $# -eq 2 ] || usage
	self_test "$2"
	exit $?
	;;
esac

[ $# -ge 1 ] && [ $# -le 2 ] || usage
check "$1" "${2-}"
exit $?
