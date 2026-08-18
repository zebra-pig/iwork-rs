# Shared plumbing for driving Pages, Numbers and Keynote from the shell.
#
# Meant to be sourced, not run. Four problems it exists to solve:
#
#   A timeout. `osascript` has none, and an app that puts up a modal dialog
#   waits for a click that is never coming — which is exactly what a document
#   the app dislikes produces. Every call therefore runs in the background and
#   is killed if it overruns.
#
#   The right application. The bundles on the machine this was written on are
#   named "Pages Creator Studio.app" and friends, so nothing here says "Pages":
#   scripts address the apps by bundle identifier — com.apple.Pages,
#   com.apple.Numbers, com.apple.Keynote — which is what an app *is*, whatever
#   its bundle was renamed to. `tell application id "com.apple.Pages"` is
#   verified working against a renamed bundle.
#
#   A way back out. A timed-out call has left a document open and possibly a
#   dialog with it, and the next call would inherit both. osa_reset closes what
#   it can and kills the app when it cannot.
#
#   One caller at a time. There is one Numbers, and every script here begins by
#   closing whatever it has open. `cargo test` runs its test binaries in
#   parallel, and three of them drive the apps — so without a lock one test
#   closes the document another has just opened, and the failure reads as "the
#   app would not open this fixture" on a document that is fine. osa_acquire is
#   the turnstile; every entry point takes it before osa_warm.

# Wait for exclusive use of the apps, and give it back on the way out.
#
# `mkdir` is the atomic primitive that exists on every system, and the pid
# inside the lock is what tells an abandoned lock from a busy one: a run killed
# at its timeout does not get to clean up, and the next caller must not wait an
# hour for a process that is gone.
osa_acquire() {
	local dir=${TMPDIR:-/tmp}/iwork-osa.lock waited=0 holder
	while ! mkdir "$dir" 2>/dev/null; do
		holder=$(cat "$dir/pid" 2>/dev/null)
		if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
			rm -rf "$dir"
			continue
		fi
		if [ "$waited" -ge "${IWORK_APP_LOCK_TIMEOUT:-1800}" ]; then
			printf 'osa: taking the app lock from %s after %ss\n' "${holder:-?}" "$waited" >&2
			rm -rf "$dir"
			continue
		fi
		sleep 1
		waited=$((waited + 1))
	done
	printf '%s' "$$" >"$dir/pid"
	OSA_LOCK=$dir
	trap 'osa_release' EXIT INT TERM
}

osa_release() {
	if [ -n "${OSA_LOCK:-}" ]; then
		rm -rf "$OSA_LOCK"
		OSA_LOCK=
	fi
}

# Bundle identifier for a document extension.
osa_bundle() {
	case "$1" in
	pages) printf '%s' 'com.apple.Pages' ;;
	numbers) printf '%s' 'com.apple.Numbers' ;;
	key) printf '%s' 'com.apple.Keynote' ;;
	*)
		printf 'not an iWork document extension: %s\n' "$1" >&2
		return 2
		;;
	esac
}

# Executable name, for the kill of last resort. The bundle may be renamed; the
# executable inside it is still called Pages/Numbers/Keynote.
osa_process() {
	case "$1" in
	pages) printf '%s' 'Pages' ;;
	numbers) printf '%s' 'Numbers' ;;
	key) printf '%s' 'Keynote' ;;
	*) return 2 ;;
	esac
}

# osa_run SECONDS [osascript arguments...]
#
# Leaves the script's output in OSA_STDOUT and OSA_STDERR rather than writing
# it, so a caller can decide what is worth printing. Returns the script's exit
# status, or 124 if it had to be killed.
osa_run() {
	local limit=$1
	shift
	local out err pid waited=0 status
	out=$(mktemp -t iwork-osa-out) || return 70
	err=$(mktemp -t iwork-osa-err) || return 70

	osascript "$@" >"$out" 2>"$err" &
	pid=$!
	while kill -0 "$pid" 2>/dev/null; do
		if [ "$waited" -ge "$limit" ]; then
			kill -9 "$pid" 2>/dev/null || true
			wait "$pid" 2>/dev/null || true
			OSA_STDOUT=$(cat "$out")
			OSA_STDERR="no answer in ${limit}s — the app is busy, or waiting on a dialog
$(cat "$err")"
			rm -f "$out" "$err"
			return 124
		fi
		sleep 1
		waited=$((waited + 1))
	done
	wait "$pid"
	status=$?
	OSA_STDOUT=$(cat "$out")
	OSA_STDERR=$(cat "$err")
	rm -f "$out" "$err"
	return "$status"
}

# Start an app and clear whatever it is already holding, so a build begins from
# the same place every time.
#
# Worth doing before anything else: a document left open by an earlier failure
# is the usual reason a script that worked yesterday hangs today, and the app
# then answers a save with -10000, or with nothing.
#
# The launch goes through `open`, not through AppleScript's `launch`, which was
# observed to return happily without starting Keynote — after which every
# command in the same script failed with -600, "Application isn't running". The
# -g -j pair keeps the app in the background and hidden, so a run does not fight
# the user for the screen.
osa_warm() {
	local extension=$1 bundle waited=0
	bundle=$(osa_bundle "$extension") || return 2
	local process
	process=$(osa_process "$extension") || return 2

	open -g -j -b "$bundle" || return 1
	while ! pgrep -x "$process" >/dev/null 2>&1; do
		if [ "$waited" -ge 30 ]; then
			printf '  %s did not start\n' "$bundle" >&2
			return 1
		fi
		sleep 1
		waited=$((waited + 1))
	done
	osa_close "$extension" 60 || osa_reset "$extension"
}

# Close whatever the app has open, one document at a time.
#
# Not `close every document saving no`, which Numbers answered with -10699 on a
# session it had restored from disk, and then refused `every document` outright
# with -1728 — while closing `document 1` in a loop went through all three
# without complaint. Whatever the plural form does differently, it is not worth
# depending on.
osa_close() {
	local extension=$1 limit=$2 bundle
	bundle=$(osa_bundle "$extension") || return 2
	osa_run "$limit" -e "tell application id \"$bundle\"
		repeat 32 times
			try
				get name of document 1
			on error
				exit repeat
			end try
			close document 1 saving no
		end repeat
	end tell"
}

# Stop an app, whatever it thinks it is doing.
#
# The blunt instrument, and the only one that reliably clears an alert: an app
# refusing a document leaves the complaint on screen, where it waits for a click
# no script can give it and blocks everything after it.
osa_kill() {
	local process
	process=$(osa_process "$1") || return 2
	pkill -x "$process" 2>/dev/null || true
	sleep 3
}

# Put an app back into a state the next call can use. Best effort by
# definition: an app showing a modal dialog will not answer an Apple event, and
# then the only thing left is to kill it.
osa_reset() {
	local extension=$1 process
	process=$(osa_process "$extension") || return 2

	if osa_close "$extension" 30; then
		return 0
	fi
	printf '  killing %s — it did not answer\n' "$process" >&2
	pkill -x "$process" 2>/dev/null || true
	sleep 3
}
