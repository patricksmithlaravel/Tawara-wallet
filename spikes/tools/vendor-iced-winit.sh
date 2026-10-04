#!/usr/bin/env bash
# Makes spikes/vendor/iced_winit: the crates.io iced_winit 0.14.1 tarball,
# checked against the checksum the application's Cargo.lock records for it,
# plus
# patches/iced_winit-0.14.1-mobile.patch. Run before any cargo command in
# spikes/. Uses cargo's download cache when it has the tarball.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
dest=$here/vendor/iced_winit
# The root Cargo.lock's entry; spikes/Cargo.lock has none, since there the
# package comes from this path.
want=7589888c8e951899cc688247a69933bb7a0511f0b4b2e122ac3fcd5dacb37f72
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cached=$(ls "${CARGO_HOME:-$HOME/.cargo}"/registry/cache/*/iced_winit-0.14.1.crate 2>/dev/null | head -1 || true)
if [ -n "$cached" ]; then
  cp "$cached" "$tmp/c.crate"
else
  curl -sSfL --retry 4 -o "$tmp/c.crate" https://static.crates.io/crates/iced_winit/iced_winit-0.14.1.crate
fi
got=$( (sha256sum "$tmp/c.crate" 2>/dev/null || shasum -a 256 "$tmp/c.crate") | cut -d' ' -f1)
[ "$got" = "$want" ] || { echo "iced_winit 0.14.1: checksum $got, expected $want" >&2; exit 1; }
rm -rf "$dest"
mkdir -p "$here/vendor"
tar -xzf "$tmp/c.crate" -C "$tmp"
mv "$tmp/iced_winit-0.14.1" "$dest"
# patch, not git apply: inside the work tree git would resolve the paths
# against the repository root.
(cd "$dest" && patch -p1 --batch --forward --quiet < "$here/patches/iced_winit-0.14.1-mobile.patch")
echo "vendor/iced_winit: crates.io 0.14.1 ($got) with the mobile patch"
