#!/usr/bin/env bash
# Manual test kit for drift: local SFTP and FTPS servers plus an isolated drift
# session. Everything it writes stays in scripts/manual-test/.state; drift runs
# with its own HOME there, so the real config and ~/.ssh/known_hosts stay
# untouched. See docs/manual-testing.md for the checklist.
set -euo pipefail

KIT=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$KIT/../.." && pwd)
STATE=$KIT/.state
export MT_STATE=$STATE MT_UID=$(id -u) MT_GID=$(id -g)

compose() { docker compose -f "$KIT/compose.yml" "$@"; }

# Docker creates missing bind-mount sources as root, and the FTPS container,
# which runs as MT_UID, then cannot write its certificate. Every command that
# starts a server creates them first.
server_dirs() { mkdir -p "$STATE/sftproot" "$STATE/ftproot" "$STATE/tls"; }

usage() {
	cat <<'USAGE'
usage: mt.sh <command>

  up                         start SFTP (127.0.0.1:2222) and FTPS (127.0.0.1:2121)
  down                       stop both servers, keep files and state
  drift                      build drift from this checkout and run it isolated
  ftps [throttle] [logins]   restart FTPS; throttle in bytes/s, max logins per IP
  rotate-cert                give the FTPS server a new certificate
  pause|unpause sftp|ftps    freeze or resume a server (keep-alive detection)
  kill ftps|sftp             stop a server abruptly (connection loss mid-transfer)
  status                     show servers and checksums of both sides
  reset                      stop servers and delete all state
USAGE
}

seed() {
	local home=$STATE/home project=$STATE/project
	server_dirs
	mkdir -p "$home"
	[ -d "$project" ] && return
	mkdir -p "$project/src/nested" "$project/assets"
	printf 'identical\n' >"$project/src/same.txt"
	printf 'line 1\nline 2\nline 3\n' >"$project/src/changed.txt"
	printf 'nested\n' >"$project/src/nested/deep.txt"
	head -c 3000000 /dev/urandom >"$project/assets/big.bin"
	printf 'hidden\n' >"$project/.hidden"
	printf 'SECRET=1\n' >"$project/.env"
	printf '.env\n' >"$project/.gitignore"
	git -C "$project" init -q

	HOME=$home XDG_CONFIG_HOME=$home/.config "$STATE/bin/drift" projects add manual-test "$project" >/dev/null
	mkdir -p -m 700 "$home/.config/drift/projects"
	cat >"$home/.config/drift/projects/manual-test.toml" <<'HOSTS'
[[hosts]]
  name = "local-sftp"
  hostname = "127.0.0.1"
  port = 2222
  user = "drift"
  root_path = "/upload"
  protocol = "sftp"
  keep_alive_interval = 5
  [hosts.auth]
    type = "password"
    password = "secret"

[[hosts]]
  name = "local-ftps"
  hostname = "127.0.0.1"
  port = 2121
  user = "drift"
  root_path = "/"
  protocol = "ftps"
  keep_alive_interval = 5
  [hosts.auth]
    type = "password"
    password = "secret"
HOSTS
	chmod 600 "$home/.config/drift/projects/manual-test.toml"
}

# checksums DIR [PATH...] hashes the files under DIR, or only under the given
# paths of it. Staging files of interrupted transfers are left out.
checksums() {
	local dir=$1
	shift
	(cd "$dir" 2>/dev/null && find "${@:-.}" -path ./.git -prune -o -type f ! -name '.*drift-tmp-*' -print0 2>/dev/null | xargs -0 -r sha256sum | sort -k2) || true
}

case ${1:-} in
up)
	server_dirs
	compose up -d --build
	;;
down) compose down ;;
drift)
	mkdir -p "$STATE/bin"
	(cd "$REPO" && go build -o "$STATE/bin/drift" .)
	seed
	cd "$STATE/project"
	exec env HOME="$STATE/home" XDG_CONFIG_HOME="$STATE/home/.config" \
		"$STATE/bin/drift" --no-dashboard --debug --log "$STATE/drift.log"
	;;
ftps)
	server_dirs
	FTPS_THROTTLE=${2:-0} FTPS_MAX_CONS=${3:-0} compose up -d --force-recreate ftps
	;;
rotate-cert)
	server_dirs
	rm -f "$STATE/tls/cert.pem" "$STATE/tls/key.pem"
	compose restart ftps
	;;
pause | unpause | kill)
	[ -n "${2:-}" ] || { usage; exit 2; }
	compose "$1" "$2"
	;;
status)
	compose ps
	# Only src/ and assets/ are synced; .hidden, .gitignore and the ignored
	# .env stay local and would make matching sides look different.
	echo "--- project (src, assets)"
	checksums "$STATE/project" ./src ./assets
	for side in sftproot ftproot; do
		echo "--- $side"
		checksums "$STATE/$side"
	done
	echo "--- staging files left behind"
	find "$STATE/sftproot" "$STATE/ftproot" -name '.*drift-tmp-*' 2>/dev/null || true
	;;
reset)
	compose down --volumes 2>/dev/null || true
	rm -rf "$STATE"
	;;
*)
	usage
	exit 2
	;;
esac
