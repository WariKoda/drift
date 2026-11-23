#!/bin/sh
# Print the CHANGELOG.md section of one release, without its heading.
# The release workflow passes the output to GoReleaser as the release text.
#
# Usage: scripts/release-notes.sh v0.2.1 [CHANGELOG.md]
set -eu

if [ $# -lt 1 ]; then
	echo "usage: $0 <tag> [changelog]" >&2
	exit 2
fi

version=${1#v}
changelog=${2:-CHANGELOG.md}

notes=$(awk -v v="$version" '
	index($0, "## [" v "]") == 1 { found = 1; next }
	found && /^## \[/ { exit }
	found { print }
' "$changelog" | sed -e '/./,$!d')

if [ -z "$notes" ]; then
	echo "$changelog has no section \"## [$version]\"" >&2
	exit 1
fi

printf '%s\n' "$notes"
