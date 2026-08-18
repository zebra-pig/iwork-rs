#!/bin/bash
#
# transition-direction-probe.sh — get a transition direction out of Keynote.
#
#   scripts/transition-direction-probe.sh [DECK.key] [OUT.key]
#
# `AnimationAttributesArchive.direction` (4) is the one transition parameter no
# probe in this repository can set: `transition settings` has four members —
# effect, duration, delay, automatic — and direction is not one of them, so every
# deck a script writes leaves the field absent. Absent is 0, "whatever this
# effect does by default", and 0 is all six decks of the corpus and all 182
# bundled themes have.
#
# There is one door left that does not need the user interface, and Keynote
# opens it itself: **PowerPoint**. `<p:push dir="u"/>` carries a direction, the
# importer has to turn it into one of Keynote's, and what it writes is the app's
# own answer. So: export a deck to `.pptx`, rewrite the `<p:transition>` element
# of every slide, open the result, save it as `.key`, and read the field.
#
# Two things about the round trip are worth knowing before reading the output.
#
#   **A transition belongs to the slide it leaves.** Keynote plays a slide's
#   transition on the way *out* of it and PowerPoint's belongs to the slide being
#   entered, so the importer shifts the deck by one: pptx slide n+1's transition
#   lands on Keynote slide n, and the last one is dropped. The table below is
#   printed already unshifted.
#
#   **Not every PowerPoint effect has a Keynote equivalent**, and the ones that
#   do not arrive as something else — `<p:cover>` comes back as `apple:slide`.
#   The direction survives regardless, which is what this probe is for.
#
# The list of directions is longer than an eight-slide deck can carry, and it
# wraps: a deck of n slides exercises the first n-1 of them. `keynote-slides`
# covers push and wipe in all four orthogonal directions, which is the part that
# matters; hand it a longer deck for the diagonals and the blinds.
#
# This is a *probe*, not a fixture builder: what it produces is evidence for
# FORMAT.md §13, not a document the test corpus depends on. Nothing it writes is
# committed.

set -u

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
# shellcheck source=lib/osa.sh
. "$here/lib/osa.sh"

deck=${1:-$repo/tests/fixtures/generated/keynote-slides.key}
out=${2:-${TMPDIR:-/tmp}/keynote-directions.key}

if [ ! -e "$deck" ]; then
	printf 'transition-direction-probe: no such deck: %s\n' "$deck" >&2
	printf '  run scripts/make-fixtures.sh first\n' >&2
	exit 2
fi
deck=$(cd "$(dirname "$deck")" && pwd)/$(basename "$deck")

work=$(mktemp -d) || exit 70
trap 'rm -rf "$work"' EXIT

# What to put on each pptx slide, from slide 2 on — slide 1's transition has no
# Keynote slide to land on. `dir` is the direction the *content* travels.
cat >"$work/patch.py" <<'PY'
import os, sys

P14 = 'xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main"'
WANTED = [
    ('push', 'r'), ('push', 'l'), ('push', 'd'), ('push', 'u'),
    ('wipe', 'r'), ('wipe', 'l'), ('wipe', 'd'), ('wipe', 'u'),
    ('cover', 'rd'), ('cover', 'ld'), ('cover', 'ru'), ('cover', 'lu'),
    ('blinds', 'horz'), ('blinds', 'vert'),
]

root = sys.argv[1]
slides = sorted(
    name for name in os.listdir(os.path.join(root, 'ppt/slides'))
    if name.startswith('slide') and name.endswith('.xml')
)
order = sorted(slides, key=lambda n: int(n[5:-4]))
for index, name in enumerate(order):
    if index == 0:
        continue                       # nothing to receive slide 1's transition
    effect, direction = WANTED[(index - 1) % len(WANTED)]
    block = ('<p:transition %s spd="med" advClick="1" p14:dur="1000">'
             '<p:%s dir="%s"/></p:transition>' % (P14, effect, direction))
    path = os.path.join(root, 'ppt/slides', name)
    text = open(path, encoding='utf-8').read()
    end = text.rindex('</p:sld>')
    head = text[:end]
    # Anything already there — a bare transition or one wrapped in an
    # mc:AlternateContent — is replaced whole.
    cut = min([i for i in (head.find('<mc:AlternateContent'),
                           head.find('<p:transition')) if i != -1] or [len(head)])
    open(path, 'w', encoding='utf-8').write(head[:cut] + block + '</p:sld>')
    print('%s\t%s\t%s' % (index, effect, direction))
PY

osa_acquire
osa_warm key || exit 1

printf 'exporting %s\n' "$(basename "$deck")"
osa_try key 900 "$here/applescript/keynote-powerpoint.applescript" \
	export "$deck" "$work/probe.pptx" || {
	printf 'transition-direction-probe: the export failed\n' >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	exit 1
}

mkdir -p "$work/unpacked"
(cd "$work/unpacked" && unzip -q "$work/probe.pptx") || exit 1
printf '\npatched, one per slide:\n'
python3 "$work/patch.py" "$work/unpacked" | sed 's/^/  slide /'
(cd "$work/unpacked" && zip -q -r -X "$work/patched.pptx" .) || exit 1

printf '\nimporting\n'
rm -rf "$out"
osa_try key 900 "$here/applescript/keynote-powerpoint.applescript" \
	import "$work/patched.pptx" "$out" || {
	printf 'transition-direction-probe: the import failed\n' >&2
	printf '%s\n' "$OSA_STDERR" | sed 's/^/  /' >&2
	exit 1
}
printf '  the app says: %s\n' "$OSA_STDOUT"
osa_close key 60 >/dev/null 2>&1 || true

printf '\nwhat Keynote wrote, in deck order:\n'
cargo run --quiet --manifest-path "$repo/Cargo.toml" --bin iwork -- \
	slides "$out" 2>/dev/null |
	grep -E '^\[|^  transition|^ +direction' |
	sed 's/^/  /'
printf '\n%s\n' "$out"
