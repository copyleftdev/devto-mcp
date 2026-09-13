#!/bin/sh
# Pick the build that matches this machine.
#
# `server.json` has no field for an operating system or a CPU architecture, and a bundle
# manifest's platform_overrides only distinguish darwin/linux/win32 — not arm64 from x86_64.
# So the bundle carries every target and the choice is made here, which keeps it one artifact
# with one checksum in the registry.
set -eu
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

case $(uname -m) in
    x86_64 | amd64)  arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *)
        echo "devto-mcp: no build for $(uname -m) on $(uname -s)." >&2
        echo "Build from source instead: cargo install --git https://github.com/copyleftdev/devto-mcp devto-mcp" >&2
        exit 1
        ;;
esac

bin="$dir/devto-mcp-$arch"
if [ ! -x "$bin" ]; then
    echo "devto-mcp: $bin is missing from the bundle." >&2
    exit 1
fi
exec "$bin" "$@"
