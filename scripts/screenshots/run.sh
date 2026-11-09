#!/usr/bin/env bash
# Regenerates the README screenshots in .github/assets/screenshots/.
#
# It builds drift from this checkout, seeds a demo project and a matching
# remote tree, starts an SFTP container on 127.0.0.1:$PORT, drives drift
# through tmux and renders each captured pane with headless Chromium.
# drift runs with its own HOME under .state/, so your real config,
# ~/.ssh/known_hosts and the Omarchy theme stay untouched.
#
# Needs: Go, Git, Docker, tmux, Python 3.11+, Chromium, ImageMagick and the
# JetBrains Mono font (Nerd Font variant preferred).
set -euo pipefail

KIT=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$KIT/../.." && pwd)
STATE=$KIT/.state
OUT=$REPO/.github/assets/screenshots
THEME=$KIT/theme.toml
PORT=${PORT:-2223}
CONTAINER=drift-screenshots-sftp
TMUX_CMD=(tmux -L drift-screenshots)

for tool in go git docker tmux python3 chromium magick; do
	command -v "$tool" >/dev/null || { echo "missing: $tool" >&2; exit 1; }
done

cleanup() {
	"${TMUX_CMD[@]}" kill-server 2>/dev/null || true
	docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}
# start COLS ROWS DIR ARGS...: runs drift in a fresh tmux pane of that size.
start() {
	local cols=$1 rows=$2 dir=$3 home=$STATE/home
	shift 3
	"${TMUX_CMD[@]}" kill-server 2>/dev/null || true
	"${TMUX_CMD[@]}" -f /dev/null new-session -d -s demo -x "$cols" -y "$rows" -c "$dir" \
		env TERM=xterm-256color COLORTERM=truecolor \
		HOME="$home" XDG_CONFIG_HOME="$home/.config" XDG_CACHE_HOME="$home/.cache" \
		XDG_STATE_HOME="$home/.local/state" DRIFT_THEME=omarchy DRIFT_THEME_FILE="$THEME" \
		"$STATE/bin/drift" "$@"
	sleep 1.5
}

keys() {
	local key
	for key in "$@"; do
		"${TMUX_CMD[@]}" send-keys -t demo "$key"
		sleep 0.4
	done
}

resize() {
	"${TMUX_CMD[@]}" resize-window -t demo -x "$1" -y "$2"
	sleep 1
}

# capture NAME TEXT...: stores the pane with its colors and its width. It
# fails unless every TEXT is on screen, so a slow connection or an error
# message never ends up in the README.
capture() {
	local name=$1 plain text
	shift
	plain=$("${TMUX_CMD[@]}" capture-pane -t demo -p)
	for text in "$@"; do
		grep -qF -- "$text" <<<"$plain" || {
			printf 'capture %s: "%s" not on screen:\n%s\n' "$name" "$text" "$plain" >&2
			exit 1
		}
	done
	"${TMUX_CMD[@]}" capture-pane -t demo -e -p >"$STATE/cap/$name.ansi"
	"${TMUX_CMD[@]}" display -p -t demo '#{pane_width}' >"$STATE/cap/$name.width"
}

# shot NAME TITLE WIDTH HEIGHT: renders a capture into $OUT/drift-NAME.png.
# WIDTH and HEIGHT size the browser window; they only need to exceed the image.
shot() {
	local html=$STATE/cap/$1.html raw=$STATE/cap/$1.raw.png
	python3 -I "$KIT/render.py" "$STATE/cap/$1.ansi" "$html" "$THEME" "$2" "$(cat "$STATE/cap/$1.width")"
	chromium --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
		--default-background-color=00000000 --window-size="$3,$4" \
		--screenshot="$raw" "file://$html" >/dev/null 2>&1
	magick "$raw" -trim +repage "$OUT/drift-$1.png"
	echo "wrote $OUT/drift-$1.png"
}

mkdir -p "$STATE/bin" "$STATE/cap"
(cd "$REPO" && go build -o "$STATE/bin/drift" .)
"$KIT/seed.sh" "$STATE" "$STATE/bin/drift" "$PORT"
trap cleanup EXIT

# Start the container after seeding: seed.sh replaces the remote directory,
# and a running container would keep the old, deleted one mounted.
docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
docker run -d --name "$CONTAINER" -p "127.0.0.1:$PORT:22" \
	-v "$STATE/remote:/home/drift/upload" atmoz/sftp:alpine "drift:secret:$(id -u)::upload" >/dev/null
sleep 2

project=$STATE/home/work/acme-shop
start 150 36 "$project" --no-dashboard

# Browser: connect to staging, open directories and mark files on both sides.
keys @ Down Enter
sleep 2
keys j l j l j l j Space j j j l j l j Space j j l j Space j Space
keys Tab j l; sleep 0.6
keys j l; sleep 0.6
keys j l; sleep 0.6
keys j j j j Space j l; sleep 0.6
keys j l; sleep 0.6
keys j j j l; sleep 0.6
keys j
resize 150 25
capture browser "REMOTE staging" "5 marked (4 local, 1 remote)" "PriceCalculator.php"

# Diff view: compare the marked files and select PriceCalculator.php.
resize 150 36
keys s
sleep 2.5
keys Tab Tab Tab Tab
sleep 1
capture diff "src/Service/PriceCalculator.php" "+11 -5" "5 pairs"

# Dashboard, started outside any project.
start 120 22 "$STATE/home" --dashboard
capture dashboard "Acme Shop" "Landing Pages"
"${TMUX_CMD[@]}" kill-server

shot browser "drift — Acme Shop" 1500 1000
shot diff "drift — Acme Shop → staging" 1500 1000
shot dashboard "drift" 1300 700
