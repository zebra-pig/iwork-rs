#!/bin/bash
#
# european-correspondent.sh — build the deck the example carries.
#
#   scripts/european-correspondent.sh [--base BASE.key] [--no-check] [OUT.key]
#
# The words are in examples/european-correspondent.rs and the theme is in the
# base deck; this script puts the two together and then asks Keynote whether it
# accepts the result.
#
#   1. a base deck, made by Keynote from its default theme — two slides, every
#      placeholder owned, both with presenter notes (see the AppleScript for why
#      that matters). --base skips this step, which is how the script runs on a
#      machine with no Keynote: hand it any .key you have.
#   2. `cargo run --example european-correspondent -- BASE OUT`, which grows the
#      deck by copying slides and writes the text into them.
#   3. scripts/app-check.sh on the result, looking for the title. This is the
#      only step that can say the deck *opens*: everything before it proves the
#      bytes are well formed, which documents that crashed Pages have also done.
#
# Without Keynote, steps 1 and 3 are skipped and the script says so — the deck
# is written, unopened by anything but this crate.

set -u

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
# shellcheck source=lib/osa.sh
. "$here/lib/osa.sh"

base=
check=1
out=

while [ $# -gt 0 ]; do
	case $1 in
	--base)
		shift
		base=$1
		;;
	--no-check) check=0 ;;
	-h | --help)
		sed -n '3,4p' "$0" | sed 's/^# \{0,1\}//'
		exit 2
		;;
	-*)
		printf 'european-correspondent: unknown option %s\n' "$1" >&2
		exit 2
		;;
	*) out=$1 ;;
	esac
	shift
done

out=${out:-$repo/The-European-Correspondent.key}

have_keynote() {
	[ "$(uname -s)" = Darwin ] && osascript -e 'id of application id "com.apple.Keynote"' \
		>/dev/null 2>&1
}

work=
cleanup() {
	[ -n "$work" ] && rm -rf "$work"
	osa_release
}
trap cleanup EXIT INT TERM

if [ -z "$base" ]; then
	if ! have_keynote; then
		printf 'european-correspondent: no Keynote here, and no --base deck given.\n' >&2
		printf '  A deck is authored from a deck that exists — see the example.\n' >&2
		printf '  Pass any .key with --base, or run this on a Mac with Keynote.\n' >&2
		exit 2
	fi
	work=$(mktemp -d) || exit 70
	base=$work/base.key
	osa_acquire
	osa_warm key
	if ! osa_try key 300 "$here/applescript/european-correspondent-base.applescript" "$base"; then
		printf 'european-correspondent: Keynote would not make the base deck\n' >&2
		printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
		osa_reset key
		exit 1
	fi
	printf 'base: %s\n' "$OSA_STDOUT"
	[ -e "$base" ] || {
		printf 'european-correspondent: Keynote said it saved %s and did not\n' "$base" >&2
		exit 1
	}
fi

rm -rf "$out"
if ! (cd "$repo" && cargo run --quiet --example european-correspondent -- "$base" "$out"); then
	printf 'european-correspondent: the deck was not written\n' >&2
	exit 1
fi

if [ "$check" = 0 ] || ! have_keynote; then
	printf 'not checked in the app — %s\n' \
		"$([ "$check" = 0 ] && echo '--no-check given' || echo 'no Keynote here')"
	exit 0
fi

# The app is the oracle, and the deck's own title is the string to look for.
"$here/app-check.sh" "$out" "The European Correspondent"
